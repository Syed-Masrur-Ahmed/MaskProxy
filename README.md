# MaskProxy

A privacy middleware layer that sits between your application and any LLM API. It intercepts outgoing requests, masks personally identifiable information (names, emails, phone numbers, SSNs, addresses), forwards the sanitized prompt to the upstream model, then rehydrates the response with the original values before returning it to the caller.

## Services

| Service  | Description                              | Port |
|----------|------------------------------------------|------|
| frontend | Next.js 15 developer dashboard           | 3000 |
| backend  | FastAPI privacy config and control API   | 8000 |
| proxy    | Rust reverse proxy (masking layer)       | 8080 |
| db       | PostgreSQL 16 — persistent config store  | 5433 |
| cache    | Redis 8 — session state and PII mapping  | 6379 |

## Requirements

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (includes Docker Compose)
- Git

No other tools need to be installed locally. Python, Node, and Rust all run inside containers.

## Setup

**1. Clone the repository**

```bash
git clone https://github.com/Syed-Masrur-Ahmed/MaskProxy.git
cd MaskProxy
```

**2. Create your environment file**

```bash
cp .env.example .env
```

For local development the default values work as-is. If you change `POSTGRES_PASSWORD`, update `DATABASE_URL` to match.

**3. Build and start all services**

```bash
docker compose up --build
```

The first build takes a few minutes — Docker is installing Python packages, Node modules, and compiling the Rust binary. Subsequent starts are faster.

**4. Open the dashboard**

Visit [http://localhost:3000](http://localhost:3000).

The backend API is available at [http://localhost:8000](http://localhost:8000) and has auto-generated docs at [http://localhost:8000/docs](http://localhost:8000/docs).

## Daily use

```bash
# Start all services (no rebuild)
docker compose up

# Start in the background
docker compose up -d

# View logs for a specific service
docker compose logs backend
docker compose logs frontend

# Stop everything
docker compose down

# Stop and delete all data volumes (full reset)
docker compose down -v
```

## Project structure

```
apps/
  frontend/   Next.js dashboard
  backend/    FastAPI config and control API
  proxy/      Rust masking proxy
packages/     Shared code (future)
docker-compose.yml
.env.example
```

## Environment variables

| Variable            | Description                              | Default                                          |
|---------------------|------------------------------------------|--------------------------------------------------|
| POSTGRES_DB         | Database name                            | maskproxy                                        |
| POSTGRES_USER       | Database user                            | maskproxy                                        |
| POSTGRES_PASSWORD   | Database password                        | changeme                                         |
| DATABASE_URL        | Full Postgres connection string          | postgresql://maskproxy:changeme@db:5432/maskproxy |
| REDIS_URL           | Redis connection string                  | redis://cache:6379/0                             |
