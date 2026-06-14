# syntax=docker/dockerfile:1
# Signaling server — standalone WebSocket matchmaking binary.
# Build context: repo root (so the Go module at infra/orchestrator/ is available).
#
# Build:
#   docker build -f infra/signaling/Dockerfile -t apeiron-signaling .
# Run:
#   docker run -p 9090:9090 -e TURN_SECRET=changeme apeiron-signaling

FROM golang:1.25 AS builder
WORKDIR /src

# Copy Go module files first (better layer caching).
COPY infra/orchestrator/go.mod infra/orchestrator/go.sum ./
RUN go mod download

COPY infra/orchestrator/ .
RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -o /bin/signaling ./cmd/signaling

# Distroless: minimal attack surface, no shell, runs as nonroot.
FROM gcr.io/distroless/static-debian12 AS signaling
COPY --from=builder /bin/signaling /signaling
EXPOSE 9090
ENTRYPOINT ["/signaling"]
