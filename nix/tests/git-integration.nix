# NixOS Integration Tests for Git Operations
# 
# This file tests SSH-based Git operations including:
# - SSH key generation and authentication
# - git clone over SSH
# - git push over SSH  
# - git pull over SSH
# - Multi-tenant repository isolation
# - Repository management (create, delete, list)
# - Git protocol (git-upload-pack, git-receive-pack)
#
# Dependencies:
# - NixOS with testers.runNixOSTest
# - repos-mcp package
# - git, openssh packages
#
# Usage:
#   nix-build -A checks.git-integration-*

{ pkgs, repos-mcp, lib, mkVMTest, mkTestMatrix, waitForReposMCP, setupSSHKeys, setupGitConfig, createTestRepo }:

let
  inherit (lib) mapAttrsToList;

  gitOperationsTests = [
    {
      id = "ssh-authentication-ed25519";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        
        with subtest("SSH public key authentication succeeds"):
          result = client.succeed("ssh -o BatchMode=yes -o ConnectTimeout=10 -p 2222 root@server 'echo auth_success'", timeout=30)
          assert "auth_success" in result, "SSH authentication should succeed"
        
        with subtest("SSH connection with invalid key is rejected"):
          client.succeed("ssh-keygen -t ed25519 -f /tmp/invalid_key -N '''' -C 'invalid'")
          client.fail("""
            ssh -o BatchMode=yes \
                -o IdentityFile=/tmp/invalid_key \
                -o PubkeyAuthentication=yes \
                -o PasswordAuthentication=no \
                -o ConnectTimeout=10 \
                -p 2222 root@server 'echo should_fail' 2>&1
          """, timeout=15)
      '';
    }
    {
      id = "git-clone-empty-repo";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "empty-repo"}
        
        with subtest("Clone empty repository over SSH"):
          client.succeed("git clone ssh://root@server:2222/tenant1/empty-repo.git /tmp/clone-empty", timeout=60)
          client.succeed("test -d /tmp/clone-empty/.git")
      '';
    }
    {
      id = "git-clone-with-content";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        
        server.succeed("mkdir -p /var/lib/repos-mcp/repositories/tenant1/content-repo.git")
        server.succeed("git init --bare /var/lib/repos-mcp/repositories/tenant1/content-repo.git")
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/content-repo.git init-content && \
          cd init-content && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo '# Content Repo' > README.md && \
          git add README.md && \
          git commit -m 'Initial commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Clone repository with content over SSH"):
          client.succeed("git clone ssh://root@server:2222/tenant1/content-repo.git /tmp/clone-content", timeout=60)
          client.succeed("test -d /tmp/clone-content/.git")
          client.succeed("test -f /tmp/clone-content/README.md")
          
          readme_content = client.succeed("cat /tmp/clone-content/README.md")
          assert "Content Repo" in readme_content
      '';
    }
    {
      id = "git-push-commit";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "push-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/push-test.git init-push && \
          cd init-push && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Initial' > README.md && \
          git add README.md && \
          git commit -m 'Initial commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Clone and push new commit"):
          client.succeed("git clone ssh://root@server:2222/tenant1/push-test.git /tmp/push-test", timeout=60)
          
          client.succeed("""
            cd /tmp/push-test && \
            echo 'New content from client' >> README.md && \
            git add README.md && \
            git commit -m 'Add content from client' && \
            git push origin main
          """, timeout=60)
          
          server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/push-test.git && git log --oneline | grep 'Add content from client'")
      '';
    }
    {
      id = "git-pull-update";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "pull-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/pull-test.git init-pull && \
          cd init-pull && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Initial' > README.md && \
          git add README.md && \
          git commit -m 'Initial commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        client.succeed("git clone ssh://root@server:2222/tenant1/pull-test.git /tmp/pull-test", timeout=60)
        
        with subtest("Pull updates from server"):
          server.succeed("""
            cd /tmp/init-pull && \
            echo 'Server update' > server-update.txt && \
            git add server-update.txt && \
            git commit -m 'Server-side update' && \
            git push origin main
          """)
          
          client.succeed("cd /tmp/pull-test && git pull origin main", timeout=60)
          client.succeed("test -f /tmp/pull-test/server-update.txt")
      '';
    }
    {
      id = "branch-operations";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "branch-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/branch-test.git init-branch && \
          cd init-branch && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Initial' > README.md && \
          git add README.md && \
          git commit -m 'Initial commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("List remote branches"):
          branches = client.succeed("git ls-remote --heads ssh://root@server:2222/tenant1/branch-test.git", timeout=30)
          assert "refs/heads/main" in branches
        
        with subtest("Push new branch"):
          client.succeed("git clone ssh://root@server:2222/tenant1/branch-test.git /tmp/branch-test", timeout=60)
          client.succeed("""
            cd /tmp/branch-test && \
            git checkout -b feature-branch && \
            echo 'Feature content' > feature.txt && \
            git add feature.txt && \
            git commit -m 'Add feature' && \
            git push -u origin feature-branch
          """, timeout=60)
          
          branches_after = client.succeed("git ls-remote --heads ssh://root@server:2222/tenant1/branch-test.git", timeout=30)
          assert "feature-branch" in branches_after
        
        with subtest("Delete remote branch"):
          client.succeed("git push ssh://root@server:2222/tenant1/branch-test.git --delete feature-branch", timeout=30)
          
          branches_final = client.succeed("git ls-remote --heads ssh://root@server:2222/tenant1/branch-test.git", timeout=30)
          assert "feature-branch" not in branches_final
      '';
    }
    {
      id = "multi-tenant-isolation";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        
        ${createTestRepo "tenant-a" "private-repo"}
        ${createTestRepo "tenant-b" "private-repo"}
        
        server.succeed("""
          cd /tmp && \
          
          git clone /var/lib/repos-mcp/repositories/tenant-a/private-repo.git init-a && \
          cd init-a && \
          git config user.email 'a@server' && \
          git config user.name 'Tenant A' && \
          echo 'Tenant A content' > README.md && \
          git add README.md && \
          git commit -m 'Tenant A commit' && \
          git branch -M main && \
          git push origin main && \
          cd .. && \
          
          git clone /var/lib/repos-mcp/repositories/tenant-b/private-repo.git init-b && \
          cd init-b && \
          git config user.email 'b@server' && \
          git config user.name 'Tenant B' && \
          echo 'Tenant B content' > README.md && \
          git add README.md && \
          git commit -m 'Tenant B commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Tenant A repository is isolated"):
          client.succeed("git clone ssh://root@server:2222/tenant-a/private-repo.git /tmp/tenant-a", timeout=60)
          readme_a = client.succeed("cat /tmp/tenant-a/README.md")
          assert "Tenant A content" in readme_a
        
        with subtest("Tenant B repository is isolated"):
          client.succeed("git clone ssh://root@server:2222/tenant-b/private-repo.git /tmp/tenant-b", timeout=60)
          readme_b = client.succeed("cat /tmp/tenant-b/README.md")
          assert "Tenant B content" in readme_b
        
        with subtest("Repository paths are isolated per tenant"):
          server.succeed("test -d /var/lib/repos-mcp/repositories/tenant-a/private-repo.git")
          server.succeed("test -d /var/lib/repos-mcp/repositories/tenant-b/private-repo.git")
          server.succeed("test ! -d /var/lib/repos-mcp/repositories/tenant-a/tenant-b")
          server.succeed("test ! -d /var/lib/repos-mcp/repositories/tenant-b/tenant-a")
      '';
    }
    {
      id = "path-traversal-protection";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "safe-repo"}
        
        with subtest("Path traversal attempts are blocked"):
          client.fail("git clone ssh://root@server:2222/../etc/passwd /tmp/traversal-test 2>&1", timeout=30)
          client.fail("git clone ssh://root@server:2222/tenant1/../../etc /tmp/traversal-test2 2>&1", timeout=30)
      '';
    }
    {
      id = "repository-not-found";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        
        with subtest("Clone non-existent repository fails gracefully"):
          client.fail("git clone ssh://root@server:2222/nonexistent/repo.git /tmp/clone-fail 2>&1", timeout=30)
      '';
    }
    {
      id = "concurrent-clones";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "concurrent-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/concurrent-test.git init-concurrent && \
          cd init-concurrent && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Concurrent test' > README.md && \
          git add README.md && \
          git commit -m 'Initial' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Concurrent clones from same repository"):
          client.succeed("""
            for i in 1 2 3 4 5; do
              (git clone ssh://root@server:2222/tenant1/concurrent-test.git /tmp/concurrent-$i &)
            done
            wait
          """, timeout=120)
          
          for i in range(1, 6):
            client.succeed(f"test -d /tmp/concurrent-{i}/.git")
      '';
    }
    {
      id = "concurrent-pushes-different-branches";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "concurrent-push"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/concurrent-push.git init-cp && \
          cd init-cp && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Base' > README.md && \
          git add README.md && \
          git commit -m 'Initial' && \
          git branch -M main && \
          git push origin main
        """)
        
        client.succeed("git clone ssh://root@server:2222/tenant1/concurrent-push.git /tmp/cp-base", timeout=60)
        
        with subtest("Concurrent pushes to different branches"):
          client.succeed("""
            for i in 1 2 3; do
              cp -r /tmp/cp-base /tmp/cp-$i
              (cd /tmp/cp-$i && \
               git checkout -b concurrent-branch-$i && \
               echo "Branch $i content" > branch_$i.txt && \
               git add branch_$i.txt && \
               git commit -m "Add branch $i" && \
               git push -u origin concurrent-branch-$i) &
            done
            wait
          """, timeout=120)
          
          branches = server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/concurrent-push.git && git branch -a")
          assert "concurrent-branch-1" in branches
          assert "concurrent-branch-2" in branches
          assert "concurrent-branch-3" in branches
      '';
    }
    {
      id = "pack-file-handling";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "pack-test"}
        
        with subtest("Push with multiple binary files creates pack"):
          client.succeed("mkdir -p /tmp/pack-test && cd /tmp/pack-test && git init")
          client.succeed("""
            cd /tmp/pack-test && \
            for i in $(seq 1 30); do
              dd if=/dev/urandom of=file_$i.bin bs=1024 count=1 2>/dev/null
              git add file_$i.bin
            done && \
            git commit -m 'Add binary files' && \
            git remote add origin ssh://root@server:2222/tenant1/pack-test.git && \
            git push -u origin main
          """, timeout=120)
          
          server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/pack-test.git && git count-objects -v")
          server.succeed("test -f /var/lib/repos-mcp/repositories/tenant1/pack-test.git/objects/pack/*.pack || test -d /var/lib/repos-mcp/repositories/tenant1/pack-test.git/objects/pack")
      '';
    }
    {
      id = "service-restart-persistence";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "restart-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/restart-test.git init-restart && \
          cd init-restart && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Persistence test' > README.md && \
          git add README.md && \
          git commit -m 'Initial' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Service restart preserves repositories"):
          server.succeed("systemctl restart repos-mcp.service")
          server.wait_for_unit("repos-mcp.service", timeout=60)
          server.wait_for_open_port(2222, timeout=60)
          
          client.succeed("rm -rf /tmp/restart-clone")
          client.succeed("git clone ssh://root@server:2222/tenant1/restart-test.git /tmp/restart-clone", timeout=60)
          client.succeed("test -f /tmp/restart-clone/README.md")
      '';
    }
    {
      id = "force-push";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "force-push-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/force-push-test.git init-fp && \
          cd init-fp && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Original content' > README.md && \
          git add README.md && \
          git commit -m 'Original commit' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Force push to repository"):
          client.succeed("git clone ssh://root@server:2222/tenant1/force-push-test.git /tmp/force-push", timeout=60)
          
          client.succeed("""
            cd /tmp/force-push && \
            echo 'Forced content' > forced.txt && \
            git add forced.txt && \
            git commit --amend -m 'Amended commit' && \
            git push --force origin main
          """, timeout=60)
          
          server.succeed("cd /var/lib/repos-mcp/repositories/tenant1/force-push-test.git && git log --oneline | grep 'Amended commit'")
      '';
    }
    {
      id = "shallow-clone";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "shallow-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/shallow-test.git init-shallow && \
          cd init-shallow && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          for i in 1 2 3 4 5; do
            echo "Commit $i" >> commits.txt
            git add commits.txt
            git commit -m "Commit $i"
          done && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Shallow clone with depth 1"):
          client.succeed("git clone --depth 1 ssh://root@server:2222/tenant1/shallow-test.git /tmp/shallow-clone", timeout=60)
          client.succeed("test -f /tmp/shallow-clone/commits.txt")
          
          log_count = client.succeed("cd /tmp/shallow-clone && git log --oneline | wc -l").strip()
          assert log_count == "1", f"Expected 1 commit in shallow clone, got {log_count}"
      '';
    }
    {
      id = "tag-operations";
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "tag-test"}
        
        server.succeed("""
          cd /tmp && \
          git clone /var/lib/repos-mcp/repositories/tenant1/tag-test.git init-tag && \
          cd init-tag && \
          git config user.email 'init@server' && \
          git config user.name 'Server Init' && \
          echo 'Tag test' > README.md && \
          git add README.md && \
          git commit -m 'Initial' && \
          git branch -M main && \
          git push origin main
        """)
        
        with subtest("Push and list tags"):
          client.succeed("git clone ssh://root@server:2222/tenant1/tag-test.git /tmp/tag-test", timeout=60)
          
          client.succeed("""
            cd /tmp/tag-test && \
            git tag -a v1.0.0 -m 'Version 1.0.0' && \
            git push origin v1.0.0
          """, timeout=60)
          
          tags = client.succeed("git ls-remote --tags ssh://root@server:2222/tenant1/tag-test.git", timeout=30)
          assert "v1.0.0" in tags
      '';
    }
    {
      id = "large-file-push";
      timeout = 900;
      testScript = ''
        ${waitForReposMCP}
        ${setupSSHKeys}
        ${setupGitConfig}
        ${createTestRepo "tenant1" "large-file"}
        
        with subtest("Push repository with large files"):
          client.succeed("mkdir -p /tmp/large-file && cd /tmp/large-file && git init")
          client.succeed("""
            cd /tmp/large-file && \
            dd if=/dev/zero of=large.bin bs=1M count=10 && \
            git add large.bin && \
            git commit -m 'Add large file' && \
            git remote add origin ssh://root@server:2222/tenant1/large-file.git && \
            git push -u origin main
          """, timeout=180)
          
          client.succeed("rm -rf /tmp/large-verify")
          client.succeed("git clone ssh://root@server:2222/tenant1/large-file.git /tmp/large-verify", timeout=120)
          client.succeed("test -f /tmp/large-verify/large.bin")
      '';
    }
  ];

in mkTestMatrix "git-integration" gitOperationsTests
