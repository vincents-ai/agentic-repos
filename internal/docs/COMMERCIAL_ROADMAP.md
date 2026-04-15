# Commercial Features Roadmap

This document outlines the commercial features for agentic-repos, designed to create clear value differentiation between the open source (AGPL) core and enterprise offerings.

## Open Source Core (AGPL)
✅ **Currently Available:**
- Basic SSH Git access (clone, push, pull)
- REST API for repository management
- Local and S3 storage backends
- Basic authentication with SSH keys
- Basic rate limiting
- Multi-tenant repository isolation

## Commercial Features by Category

### 🔐 Enterprise Git Management

#### Authentication & Identity (3 features)
- **LDAP/Active Directory Integration** - Enterprise directory service integration
- **SAML SSO** - Single sign-on for enterprise identity providers  
- **OAuth2/OIDC** - Modern authentication with popular providers (Google, GitHub, etc.)

#### Governance & Control (4 features)
- **Audit Logging** - Comprehensive logging of all Git operations and API calls
- **Access Tracking** - Tamper-proof logs with change tracking and compliance trails
- **Repository Templates** - Standardized repository creation with enforced policies
- **Custom Webhooks** - Integration system for external tools and notifications

### ⚡ Performance & Scale (5 features)

#### Infrastructure (4 features)
- **Multi-Region Replication** - Global repository distribution for performance
- **Horizontal Scaling** - Multi-node deployment with shared state management
- **Redis Caching** - High-performance caching layer for Git operations and API
- **Load Balancing** - Intelligent request routing and traffic distribution

#### Monitoring (1 feature)
- **Performance Monitoring** - Prometheus/Grafana integration with custom metrics

### 🛡️ Security & Compliance (6 features)

#### Security (2 features)
- **Security Scanning** - Automated vulnerability detection in repository contents
- **Role-Based Access Control** - Fine-grained permissions with custom roles

#### Compliance (3 features)
- **SOC2 Compliance** - Controls and reports for service organization compliance
- **GDPR Compliance** - Data protection features for European regulations
- **HIPAA Compliance** - Healthcare data protection and audit capabilities

#### Business Continuity (1 feature)
- **Backup & Disaster Recovery** - Automated backup with point-in-time recovery

### 🚀 Developer Experience (6 features)

#### User Interface (1 feature)
- **Web UI** - Full repository management interface with admin controls

#### CI/CD Integration (3 features)
- **GitHub Actions** - Native integration with GitHub's CI/CD platform
- **GitLab CI** - Integration with GitLab pipelines and deployment
- **Jenkins** - Enterprise CI/CD pipeline integration

#### API & Analytics (2 features)
- **GraphQL API** - Advanced querying capabilities with custom fields
- **Repository Analytics** - Usage insights, performance metrics, and trend analysis

## Pricing Tier Suggestions

### Free Tier (AGPL)
- Up to 5 repositories
- Basic features only
- Community support

### Pro Tier ($29/month/user)
- Unlimited repositories
- Authentication integrations (LDAP/SAML/OAuth)
- Basic webhooks and monitoring
- Email support

### Enterprise Tier ($99/month/user)
- Everything in Pro
- Security scanning and compliance features
- Advanced scaling and caching
- Web UI and analytics
- Priority support with SLA

### Enterprise Plus (Custom pricing)
- Everything in Enterprise
- Multi-region replication
- Custom compliance requirements
- Dedicated support and consulting
- On-premises deployment options

## Implementation Priority

**Note**: This is an MCP-first, agent-focused Git host for agentic LLMs. Human-facing features (Web UI, analytics dashboards) are deprioritized in favor of programmatic access and API capabilities.

### Phase 1 (High Value, Lower Complexity - Agent-Focused)
1. RBAC - Security is table stakes for enterprise
2. Audit Logging - Compliance requirement for most enterprises
3. Basic webhooks - Integration capability for agentic workflows
4. GraphQL API - Advanced programmatic access for AI agents

### Phase 2 (Core Enterprise Features)
1. SAML/OAuth authentication - Enterprise identity integration for agent authentication
2. Security scanning - Automated security checks for agent-managed repositories
3. Performance monitoring - API performance metrics for agent workflows
4. Backup & recovery - Business continuity

### Phase 3 (Scale & Advanced Features)
1. Redis caching - Performance at scale for high-frequency agent operations
2. Horizontal scaling - Enterprise scale requirements
3. Compliance reports (SOC2/GDPR) - Regulatory requirements
4. CI/CD integrations - GitHub Actions, GitLab CI for automated agent workflows

### Phase 4 (Advanced Enterprise)
1. Multi-region replication - Global enterprises
2. Advanced analytics - Repository usage patterns, agent activity insights
3. HIPAA compliance - Specialized verticals
4. Custom MCP protocols - Extended MCP capabilities for specialized agent workflows

### Deprioritized Features (Human-Facing)
- Web UI - Not needed for agent-first platform
- Repository browsing interfaces - Agents use programmatic access
- Visual analytics dashboards - Metrics exposed via API instead

## Market Differentiation

This feature set positions agentic-repos to compete with:
- **GitHub Enterprise**: Similar security and compliance features
- **GitLab**: Comparable CI/CD integration and enterprise features  
- **Bitbucket**: Enterprise authentication and scaling capabilities
- **Azure DevOps**: Performance monitoring and analytics

**Key Differentiators:**
- **MCP-first architecture** - Built for agentic LLMs, not humans
- **Programmatic access** - GraphQL + REST APIs optimized for agents
- Open source core (vendor independence)
- Multi-cloud storage (avoid lock-in)
- Compliance-first design
- Performance-optimized architecture