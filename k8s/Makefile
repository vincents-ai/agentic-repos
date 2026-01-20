# Makefile for Kubernetes deployments

.PHONY: k8s-deploy-local k8s-deploy-staging k8s-deploy-production k8s-delete-all k8s-logs

# Deploy to local (Garage)
k8s-deploy-local: ## Deploy to local Kubernetes with Garage
	kubectl apply -k k8s/overlays/local

# Deploy to staging
k8s-deploy-staging: ## Deploy to staging Kubernetes with SeaweedFS
	kubectl apply -k k8s/overlays/staging

# Deploy to production
k8s-deploy-production: ## Deploy to production Kubernetes with SeaweedFS + S3
	kubectl apply -k k8s/overlays/production

# Delete all resources
k8s-delete-all: ## Delete all ogit Kubernetes resources
	kubectl delete namespace ogit

# View logs
k8s-logs: ## View ogit pod logs
	kubectl logs -n ogit -l app.kubernetes.io/name=ogit -f

# Port forward for local development
k8s-port-forward: ## Port forward to local deployment
	kubectl port-forward -n ogit svc/ogit 8080:80 3000:3000

# Build and push Docker image
docker-build: ## Build Docker image
	docker build -t ogit:latest -f Dockerfile.rust .

docker-push: ## Push Docker image to registry
	docker tag ogit:latest $$(cat .docker-repo)/ogit:latest
	docker push $$(cat .docker-repo)/ogit:latest

# Kustomize build (preview)
k8s-preview-local: ## Preview local K8s manifests
	kustomize build k8s/overlays/local

k8s-preview-production: ## Preview production K8s manifests
	kustomize build k8s/overlays/production
