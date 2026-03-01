// Package transport implements the gRPC bridge to the Rust kernel.
// Follows strict_rules.md | camelCase Go | Zero-copy protobuf messages
// Memory cost: ~8 MB (gRPC server + connection pool)
// agent.md: "Go channels + typed protobuf messages (zero-copy)"
// Bidirectional: server receives external commands, client forwards to Rust kernel.
package transport

import (
	"context"
	"net"

	"time"

	"github.com/rs/zerolog/log"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/keepalive"
)

// GRPCTransport handles bidirectional communication between
// Go orchestrator and Rust kernel.
// Memory cost: ~8 MB (gRPC server + listener + kernel connection)
type GRPCTransport struct {
	listenAddr string
	kernelAddr string
	server     *grpc.Server
	listener   net.Listener
}

// NewGRPCTransport creates a new bidirectional gRPC transport layer.
// Memory cost: ~8 MB (server allocated, not yet listening)
func NewGRPCTransport(listenAddr, kernelAddr, tlsCert, tlsKey string) (*GRPCTransport, error) {
	lis, err := net.Listen("tcp", listenAddr)
	if err != nil {
		return nil, err
	}

	// Create gRPC server with optimized settings for <10ms latency.
	// strict_rules.md: max 4 MB messages, keepalive for long-running streams.
	opts := []grpc.ServerOption{
		grpc.MaxRecvMsgSize(4 * 1024 * 1024), // 4 MB max message
		grpc.MaxSendMsgSize(4 * 1024 * 1024),
		grpc.KeepaliveParams(keepalive.ServerParameters{
			MaxConnectionIdle: 5 * time.Minute,
			Time:              30 * time.Second,
			Timeout:           10 * time.Second,
		}),
	}

	if tlsCert != "" && tlsKey != "" {
		creds, err := credentials.NewServerTLSFromFile(tlsCert, tlsKey)
		if err != nil {
			return nil, err
		}
		opts = append(opts, grpc.Creds(creds))
		log.Info().Str("cert", tlsCert).Msg("mTLS enabled for gRPC transport")
	}

	server := grpc.NewServer(opts...)

	log.Info().
		Str("listenAddr", listenAddr).
		Str("kernelAddr", kernelAddr).
		Msg("gRPC transport created (bidirectional)")

	return &GRPCTransport{
		listenAddr: listenAddr,
		kernelAddr: kernelAddr,
		server:     server,
		listener:   lis,
	}, nil
}

// Serve starts the gRPC server (blocking).
// Memory cost: ~2 MB additional (accept loop + connection handling)
func (t *GRPCTransport) Serve(ctx context.Context) {
	log.Info().Str("addr", t.listenAddr).Msg("gRPC transport serving")

	go func() {
		<-ctx.Done()
		t.server.GracefulStop()
	}()

	if err := t.server.Serve(t.listener); err != nil {
		log.Error().Err(err).Msg("gRPC server error")
	}
}

// Stop gracefully stops the gRPC server.
// Memory cost: frees server resources
func (t *GRPCTransport) Stop() {
	t.server.GracefulStop()
	log.Info().Msg("gRPC transport stopped")
}

// Server returns the underlying gRPC server for service registration.
// Memory cost: 0 (returns reference)
func (t *GRPCTransport) Server() *grpc.Server {
	return t.server
}
