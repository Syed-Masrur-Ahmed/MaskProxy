FROM node:22-alpine

WORKDIR /app

# Default backend URL — resolvable inside the Docker network.
# Override with BACKEND_URL env var if the service name changes.
ENV BACKEND_URL=http://backend:8000

# Install dependencies first for layer caching
COPY package.json package-lock.json ./
RUN npm ci

# Copy source — in dev this layer is overridden by the volume mount
COPY . .

EXPOSE 3000

CMD ["npm", "run", "dev"]
