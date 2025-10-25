markdown# Docker Development Guide

## Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+
- Make (optional, for using Makefile commands)

## Quick Start

1. Clone the repository:
```bash
git clone https://github.com/antinomyhq/forge.git
cd forge

Copy the environment template:

bashcp .env.example .env
# Edit .env with your API keys

Build and run:

bash# Using Make
make dev

# Or using docker-compose directly
docker-compose up --build
Development Workflow
Building the Image
bashmake build
# or
docker build -t forge:latest .
Running the Container
bash# Interactive mode
make run

# Or with specific command
docker run -it --rm forge:latest --help
Development Mode
Use docker-compose for a full development environment:
bashmake dev
This will:

Mount your local workspace
Set up persistent volumes for config and cache
Load environment variables from .env
Keep the container running for interactive use

Running Tests
bashmake test
Accessing the Container
bashmake shell
# or
docker exec -it forge-app /bin/bash