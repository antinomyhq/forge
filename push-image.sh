#!/bin/bash

# Script to build and push the forge-eval Docker image to a registry
# Usage: ./push-image.sh [registry/username]

set -e

# Check if registry/username is provided
if [ -z "$1" ]; then
    echo "Usage: ./push-image.sh [registry/username]"
    echo ""
    echo "Examples:"
    echo "  ./push-image.sh yourusername              # Docker Hub"
    echo "  ./push-image.sh ghcr.io/yourusername      # GitHub Container Registry"
    echo ""
    exit 1
fi

REGISTRY_USER="$1"
IMAGE_NAME="forge-eval"
TAG="latest"
FULL_IMAGE="${REGISTRY_USER}/${IMAGE_NAME}:${TAG}"

echo "🏗️  Building Docker image..."
docker build -f Dockerfile.eval -t "${IMAGE_NAME}:${TAG}" .

echo "🏷️  Tagging image as ${FULL_IMAGE}..."
docker tag "${IMAGE_NAME}:${TAG}" "${FULL_IMAGE}"

echo "📤 Pushing to registry..."
docker push "${FULL_IMAGE}"

echo ""
echo "✅ Image pushed successfully!"
echo ""
echo "📝 Next steps:"
echo "   1. Update benchmarks/daytona-orchestrator.ts"
echo "   2. Change the customImage to: \"${FULL_IMAGE}\""
echo "   3. Run: npm run eval ./evals/echo/task.yml --distributed"
echo ""
