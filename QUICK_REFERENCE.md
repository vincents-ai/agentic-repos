# repos-mcp Quick Reference Card

## Build & Package

```bash
# Build plugin
cd ~/code/vincents-ai/repos-mcp
nix build

# Create package
./package.sh
# → dist/repos-mcp-plugin-0.1.0.tar.gz
```

## Deploy

```bash
# Extract and install
tar -xzf dist/repos-mcp-plugin-0.1.0.tar.gz
cd repos-mcp-plugin-0.1.0
export SKYLET_DIR=~/code/skylet
./install.sh

# Setup SSH
mkdir -p $SKYLET_DIR/data/ssh
ssh-keygen -t rsa -f $SKYLET_DIR/data/ssh/host_key -N ""
cat ~/.ssh/id_rsa.pub >> $SKYLET_DIR/data/ssh/authorized_keys

# Start
cd $SKYLET_DIR
source repos-mcp.env
./target/release/skylet server
```

## Test

```bash
# Basic test
./test-simple.sh

# Integration test
./test-integration.sh

# SSH test (requires running Skylet)
./test-ssh-integration.sh
```

## SSH Git Operations

```bash
# Clone
git clone ssh://git@localhost:2222/tenant-id/repo-name.git

# Push
git push origin master

# Pull
git pull origin master
```

## Plugin Actions

```bash
# Health check
plugin-test-harness execute -p plugin.so -a health --args '{}'

# Create repo
plugin-test-harness execute -p plugin.so -a repo.create \
  --args '{"id":"1","method":"repo.create","params":{"name":"test","tenant_id":"t1"}}'

# List repos
plugin-test-harness execute -p plugin.so -a repo.list \
  --args '{"id":"2","method":"repo.list","params":{"tenant_id":"t1"}}'
```

## Configuration

```bash
# SSH (REQUIRED)
export REPOS_SSH_ENABLED=true
export REPOS_SSH_PORT=2222
export REPOS_SSH_HOST_KEY_PATH="./data/ssh/host_key"
export REPOS_SSH_AUTHORIZED_KEYS_PATH="./data/ssh/authorized_keys"

# Storage
export REPOS_STORAGE_PATH="./data/repos"
export REPOS_STORAGE_BACKEND="local"  # or "s3"

# S3 (optional)
export GARAGE_ENDPOINT="http://localhost:3900"
export GARAGE_ACCESS_KEY_ID="garage"
export GARAGE_SECRET_ACCESS_KEY="garage"
```

## Troubleshooting

```bash
# Check SSH server
netstat -tuln | grep 2222

# Check plugin
ldd plugins/librepos_mcp.so

# Check logs
tail -f skylet.log | grep repos_mcp

# Test SSH
ssh -T git@localhost -p 2222
```

## Docs

- DEPLOYMENT.md - Full deployment guide
- docs/SSH_SETUP.md - SSH configuration
- AGENTS.md - AI agent guide
- FINAL_SUMMARY.md - Complete summary

## Status

✅ Plugin validated  
✅ Git operations working  
✅ SSH enabled by default  
✅ LFS configured  
✅ Multi-tenant ready  
✅ Skylet integration tested  
✅ garage-s3 ready  
✅ Packaged for deployment  

**Version**: 0.1.0 | **Date**: 2026-03-07 | **Status**: READY
