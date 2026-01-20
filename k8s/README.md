# README - Kubernetes Deployment

## Quick Start

### Local Development (Garage)
```bash
# Deploy Garage + oGit
make k8s-deploy-local

# Port forward for access
make k8s-port-forward
```

### Staging/Production (SeaweedFS + S3)
```bash
# Configure AWS credentials
kubectl create secret generic aws-credentials \
  -n ogit --from-literal=AWS_ACCESS_KEY_ID=xxx \
  --from-literal=AWS_SECRET_ACCESS_KEY=xxx

# Deploy SeaweedFS + oGit
make k8s-deploy-production
```

## Directory Structure

```
k8s/
├── base/                          # Base manifests
│   ├── namespace.yaml
│   ├── configmap.yaml
│   ├── secrets.yaml
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── ingress.yaml
│   ├── pdb.yaml
│   └── hpa.yaml
├── overlays/
│   ├── local/                     # Garage (local dev)
│   │   ├── kustomization.yaml
│   │   └── garage.yaml
│   ├── staging/                   # SeaweedFS (staging)
│   │   ├── kustomization.yaml
│   │   └── seaweedfs.yaml
│   └── production/                # SeaweedFS + S3 (prod)
│       ├── kustomization.yaml
│       └── seaweedfs.yaml
├── Makefile                       # Deployment commands
└── README.md                      # This file
```

## Environment Configuration

### Local (Garage)
- **Storage**: Garage S3-compatible storage
- **Cache**: Disabled
- **Purpose**: Development and testing

### Production (SeaweedFS + S3)
- **Storage**: SeaweedFS gateway → AWS S3
- **Cache**: Enabled (50GB local cache)
- **Purpose**: Production workloads

## Prerequisites

- Kubernetes 1.19+ (1.23+ for full Gateway API support)
- kubectl with kustomize
- **Gateway API**: Envoy Gateway or other Gateway controller
- Cert-manager for TLS (production)

### Install Gateway API Controller

**Option 1: Envoy Gateway (recommended)**
```bash
kubectl apply --server-side -f https://github.com/envoyproxy/gateway/releases/download/v1.6.2/install.yaml
```

**Option 2: Cilium**
```bash
helm install cilium cilium/cilium \
  --namespace kube-system \
  --set gatewayAPI.enabled=true
```

### Install Cert-Manager (for TLS)

```bash
helm install cert-manager jetstack/cert-manager \
  --namespace cert-manager \
  --create-namespace \
  --set installCRDs=true
```

## Scaling

The base deployment includes HPA configured for:
- Min replicas: 2
- Max replicas: 10
- CPU target: 70%
- Memory target: 80%

## Monitoring

The deployment includes:
- Liveness/readiness probes
- Resource requests/limits
- Pod disruption budget
- Horizontal pod autoscaler

## Security

- Non-root user (UID 1000)
- Read-only root filesystem
- Dropped capabilities
- Secret management via Kubernetes secrets
