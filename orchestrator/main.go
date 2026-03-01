// main.go – Agenticos Orchestrator Entry Point
// Follows strict_rules.md | camelCase Go | Zero trust
// Memory budget: ≤ 320 MB total (30 MB base + 50 agents × ~5 MB each)
// Connects to Rust kernel via gRPC for agent lifecycle management.
// Exposes REST API + WebSocket dashboard via Fiber.
// No Python in hot path – Go handles orchestration, Rust handles execution.
//
// Usage:
//   agenticos-orchestrator --version
//   agenticos-orchestrator daemon
//   agenticos-orchestrator run "Mera startup plan banao"

package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/agenticos/orchestrator/pkg/api"
	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/agenticos/orchestrator/pkg/supervisor"
	"github.com/agenticos/orchestrator/pkg/transport"
	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
)

// version is set at build time via -ldflags
// Memory cost: ~64 bytes (static string)
var version = "0.1.0"

// config holds orchestrator configuration.
// Memory cost: ~256 bytes (struct on stack)
type config struct {
	kernelAddr  string
	grpcAddr    string
	apiAddr     string
	tlsCertFile string
	tlsKeyFile  string
	maxAgents   int
	showVersion bool
}

// parseConfig parses command-line flags.
// Memory cost: ~256 bytes (config struct)
func parseConfig() config {
	cfg := config{}
	flag.StringVar(&cfg.kernelAddr, "kernel-addr", "127.0.0.1:50051", "Rust kernel gRPC address")
	flag.StringVar(&cfg.grpcAddr, "grpc-addr", "0.0.0.0:50052", "Orchestrator gRPC listen address")
	flag.StringVar(&cfg.apiAddr, "api-addr", "0.0.0.0:8080", "REST API + Dashboard address")
	flag.StringVar(&cfg.tlsCertFile, "tls-cert", "", "Path to TLS certificate file")
	flag.StringVar(&cfg.tlsKeyFile, "tls-key", "", "Path to TLS private key file")
	flag.IntVar(&cfg.maxAgents, "max-agents", 50, "Maximum concurrent agents")
	flag.BoolVar(&cfg.showVersion, "version", false, "Show version and exit")
	flag.Parse()
	return cfg
}

func main() {
	// --- Initialize structured logging ---
	// Memory cost: ~2 KB (zerolog writer)
	zerolog.TimeFieldFormat = zerolog.TimeFormatUnix
	log.Logger = log.Output(zerolog.ConsoleWriter{Out: os.Stderr, TimeFormat: "15:04:05"})

	cfg := parseConfig()

	if cfg.showVersion {
		fmt.Printf("agenticos-orchestrator v%s | Follows strict_rules.md | RAM ≤ 320 MB\n", version)
		os.Exit(0)
	}

	// Check for subcommand
	args := flag.Args()
	subcommand := ""
	goalArg := ""
	if len(args) > 0 {
		subcommand = args[0]
		if len(args) > 1 {
			goalArg = strings.Join(args[1:], " ")
		}
	}

	bootStart := time.Now()

	log.Info().
		Str("version", version).
		Str("kernelAddr", cfg.kernelAddr).
		Str("grpcAddr", cfg.grpcAddr).
		Str("apiAddr", cfg.apiAddr).
		Int("maxAgents", cfg.maxAgents).
		Msg("⚡ Agenticos orchestrator starting")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// --- Initialize Kernel Client ---
	// Memory cost: ~4 MB (gRPC connection + agent state)
	kernelClient := kernel.NewClient(cfg.kernelAddr, cfg.maxAgents, cfg.tlsCertFile, cfg.tlsKeyFile)
	if err := kernelClient.Connect(ctx); err != nil {
		log.Warn().Err(err).Msg("kernel connection failed – running in standalone mode")
	}
	log.Info().Msg("kernel client initialized")

	// --- Initialize Supervisor Tree ---
	// Memory cost: ~4 MB (tree metadata + agent slots)
	supervisorTree := supervisor.NewTree(cfg.maxAgents)
	go supervisorTree.Run(ctx)
	log.Info().Int("capacity", cfg.maxAgents).Msg("supervisor tree initialized")

	// --- Initialize gRPC Transport ---
	// Memory cost: ~8 MB (gRPC server + connection pool)
	grpcTransport, err := transport.NewGRPCTransport(cfg.grpcAddr, cfg.kernelAddr, cfg.tlsCertFile, cfg.tlsKeyFile)
	if err != nil {
		log.Fatal().Err(err).Msg("failed to initialize gRPC transport")
	}
	go grpcTransport.Serve(ctx)
	log.Info().Str("addr", cfg.grpcAddr).Msg("gRPC transport started")

	// --- Initialize REST API + Dashboard ---
	// Memory cost: ~8 MB (Fiber + WebSocket hub)
	apiServer := api.NewServer(cfg.apiAddr, kernelClient, supervisorTree, cfg.tlsCertFile, cfg.tlsKeyFile)
	apiServer.Start(ctx)
	log.Info().Str("addr", cfg.apiAddr).Msg("REST API + Dashboard started")

	bootElapsed := time.Since(bootStart)
	log.Info().
		Dur("bootTime", bootElapsed).
		Msg("⚡ Agenticos orchestrator fully booted")

	// --- Handle subcommand ---
	switch subcommand {
	case "run":
		if goalArg == "" {
			fmt.Println("Usage: agenticos-orchestrator run \"<goal>\"")
			os.Exit(1)
		}

		log.Info().Str("goal", goalArg).Msg("🚀 Running one-shot agent")

		snapshot, err := kernelClient.RunAgent(ctx, goalArg, "high")
		if err != nil {
			log.Fatal().Err(err).Msg("agent run failed")
		}

		fmt.Println()
		fmt.Println("═══ Agenticos Agent Result ═══")
		fmt.Printf("  Agent ID:    %s\n", snapshot.ID)
		fmt.Printf("  Goal:        %s\n", snapshot.Goal)
		fmt.Printf("  State:       %s\n", snapshot.State)
		fmt.Printf("  Priority:    %s\n", snapshot.Priority)
		fmt.Printf("  Transitions: %d\n", snapshot.TransitionCount)
		fmt.Printf("  Memory:      %.2f MB\n", float64(snapshot.MemoryUsageBytes)/(1024*1024))
		fmt.Println("══════════════════════════════")

		metrics := kernelClient.GetMetrics(ctx)
		fmt.Printf("  Active: %d | Spawned: %d | Completed: %d\n",
			metrics.AgentsActive, metrics.AgentsTotalSpawned, metrics.AgentsTotalCompleted)
		fmt.Println("══════════════════════════════")
		fmt.Println()

		// Keep running so dashboard shows results
		log.Info().
			Str("dashboard", "http://"+cfg.apiAddr+"/dashboard").
			Msg("Dashboard available – press Ctrl+C to exit")

	case "daemon", "":
		if subcommand == "daemon" {
			log.Info().Msg("Entering daemon mode")
		}
		log.Info().
			Str("dashboard", "http://"+cfg.apiAddr+"/dashboard").
			Str("api", "http://"+cfg.apiAddr+"/api/agents").
			Msg("📊 Dashboard & API ready")

	default:
		fmt.Printf("Unknown command: %s\n", subcommand)
		fmt.Println("Usage: agenticos-orchestrator [daemon|run \"<goal>\"] [flags]")
		os.Exit(1)
	}

	// --- Graceful shutdown ---
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	sig := <-sigCh

	log.Info().Str("signal", sig.String()).Msg("shutdown signal received")
	cancel()
	grpcTransport.Stop()
	log.Info().Msg("⚡ Agenticos orchestrator shut down gracefully")
}
