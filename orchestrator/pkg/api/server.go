// Package api implements the Fiber-based REST + WebSocket server.
// Follows strict_rules.md | camelCase Go
// Memory cost: ~8 MB (Fiber engine + WebSocket hub + route handlers)
// Endpoints: GET /agents, POST /spawn, POST /kill/:id, GET /metrics,
//
//	GET /dashboard, WS /ws
//
// All responses JSON. WebSocket broadcasts agent events in real-time.
// Security: JWT auth on /api/* routes, rate limiting on all routes.
package api

import (
	"context"
	"sync"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/fiber/v2/middleware/cors"
	"github.com/gofiber/fiber/v2/middleware/logger"
	"github.com/gofiber/fiber/v2/middleware/requestid"
	"github.com/gofiber/websocket/v2"
	"github.com/rs/zerolog/log"

	"github.com/agenticos/orchestrator/pkg/auth"
	"github.com/agenticos/orchestrator/pkg/integrations"
	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/agenticos/orchestrator/pkg/ratelimit"
	"github.com/agenticos/orchestrator/pkg/supervisor"
)

// Server is the Fiber-based REST + WebSocket API server.
// Memory cost: ~8 MB (Fiber + WebSocket connections)
type Server struct {
	app            *fiber.App
	kernelClient   *kernel.Client
	supervisorTree *supervisor.Tree
	listenAddr     string
	wsClients      map[*websocket.Conn]bool
	wsMu           sync.RWMutex
	authMiddleware *auth.Middleware
	tlsCertFile    string
	tlsKeyFile     string
}

// NewServer creates the API server with security middleware.
// Memory cost: ~8 MB
func NewServer(listenAddr string, kc *kernel.Client, st *supervisor.Tree, tlsCert, tlsKey string) *Server {
	app := fiber.New(fiber.Config{
		AppName:               "UGHI Orchestrator v1.0.0",
		ServerHeader:          "UGHI",
		DisableStartupMessage: true,
		ReadTimeout:           10 * time.Second,
		WriteTimeout:          10 * time.Second,
		BodyLimit:             4 * 1024 * 1024, // 4 MB max body
	})

	// --- JWT Auth Middleware ---
	// Reads UGHI_JWT_SECRET from env. Disabled gracefully if not set.
	authMw := auth.New(auth.Config{
		Enabled: true,
		SkipPaths: []string{
			"/health",
			"/dashboard",
			"/ws",
			"/",
		},
	})

	s := &Server{
		app:            app,
		kernelClient:   kc,
		supervisorTree: st,
		listenAddr:     listenAddr,
		wsClients:      make(map[*websocket.Conn]bool),
		authMiddleware: authMw,
		tlsCertFile:    tlsCert,
		tlsKeyFile:     tlsKey,
	}

	// --- Global Middleware Stack ---
	// 1. Request ID (propagated via X-Request-ID header)
	app.Use(requestid.New())

	// 2. CORS
	app.Use(cors.New())

	// 3. Request logging with request ID
	app.Use(logger.New(logger.Config{
		Format:     "${time} | ${locals:requestid} | ${status} | ${latency} | ${method} ${path}\n",
		TimeFormat: "15:04:05",
	}))

	// 3. Rate limiting (100 req/min default, applied globally)
	readLimiter := ratelimit.New(ratelimit.DefaultConfig())
	app.Use(readLimiter.Handler())

	// REST endpoints
	setupRoutes(app, s)

	return s
}

// setupRoutes registers all REST and WebSocket routes.
func setupRoutes(app *fiber.App, s *Server) {
	// Health check (no auth required)
	app.Get("/health", s.handleHealth)

	// API group with JWT auth + stricter rate limit for writes
	api := app.Group("/api", s.authMiddleware.Handler())

	// Read endpoints (viewer role)
	api.Get("/agents", auth.RequireRole(auth.RoleViewer), s.handleListAgents)
	api.Get("/monitor/:id", auth.RequireRole(auth.RoleViewer), s.handleMonitor)
	api.Get("/metrics", auth.RequireRole(auth.RoleViewer), s.handleMetrics)
	api.Get("/tree", auth.RequireRole(auth.RoleViewer), s.handleTree)

	// Write endpoints (operator role + stricter rate limit)
	writeLimiter := ratelimit.New(ratelimit.WriteConfig())
	api.Post("/spawn", writeLimiter.Handler(), auth.RequireRole(auth.RoleOperator), s.handleSpawn)
	api.Post("/kill/:id", writeLimiter.Handler(), auth.RequireRole(auth.RoleOperator), s.handleKill)
	api.Post("/transition", writeLimiter.Handler(), auth.RequireRole(auth.RoleOperator), s.handleTransition)
	api.Post("/run", writeLimiter.Handler(), auth.RequireRole(auth.RoleOperator), s.handleRunAgent)

	// Admin endpoints
	api.Post("/token", auth.RequireRole(auth.RoleAdmin), s.handleGenerateToken)

	// WebSocket (no auth – uses separate token validation)
	app.Use("/ws", func(c *fiber.Ctx) error {
		if websocket.IsWebSocketUpgrade(c) {
			return c.Next()
		}
		return fiber.ErrUpgradeRequired
	})
	app.Get("/ws", websocket.New(s.handleWebSocket))

	// Chat Integrations Webhooks (Public access, platform specific validation internal)
	webhooks := app.Group("/api/webhooks")
	webhooks.Post("/telegram", integrations.HandleTelegram(s.kernelClient))
	webhooks.Post("/discord", integrations.HandleDiscord(s.kernelClient))
	webhooks.Post("/slack", integrations.HandleSlack(s.kernelClient))

	// Dashboard (HTML, no auth)
	app.Get("/dashboard", s.handleDashboard)
	app.Get("/", func(c *fiber.Ctx) error {
		return c.Redirect("/dashboard")
	})
}

// handleGenerateToken generates a JWT token (admin only).
// POST /api/token { "sub": "user@example.com", "role": "operator", "ttl_hours": 24 }
func (s *Server) handleGenerateToken(c *fiber.Ctx) error {
	var req struct {
		Sub      string `json:"sub"`
		Role     string `json:"role"`
		TTLHours int    `json:"ttl_hours"`
	}
	if err := c.BodyParser(&req); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "invalid request body"})
	}
	if req.Sub == "" || req.Role == "" {
		return c.Status(400).JSON(fiber.Map{"error": "sub and role are required"})
	}
	if req.TTLHours <= 0 {
		req.TTLHours = 24
	}

	token, err := s.authMiddleware.GenerateToken(
		req.Sub,
		auth.Role(req.Role),
		time.Duration(req.TTLHours)*time.Hour,
	)
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}

	return c.JSON(fiber.Map{
		"token":      token,
		"expires_in": req.TTLHours * 3600,
		"role":       req.Role,
	})
}

// Start begins serving the API (non-blocking).
// Memory cost: ~2 MB (accept loop)
func (s *Server) Start(ctx context.Context) {
	// Start WebSocket broadcaster
	go s.broadcastEvents(ctx)

	go func() {
		log.Info().Str("addr", s.listenAddr).Msg("API server starting")
		var err error
		if s.tlsCertFile != "" && s.tlsKeyFile != "" {
			log.Info().Str("cert", s.tlsCertFile).Msg("TLS enabled for REST API")
			err = s.app.ListenTLS(s.listenAddr, s.tlsCertFile, s.tlsKeyFile)
		} else {
			err = s.app.Listen(s.listenAddr)
		}

		if err != nil {
			log.Error().Err(err).Msg("API server error")
		}
	}()

	go func() {
		<-ctx.Done()
		_ = s.app.Shutdown()
		log.Info().Msg("API server stopped")
	}()
}

// broadcastEvents listens for kernel events and broadcasts to WebSocket clients.
// Memory cost: ~1 KB per broadcast
func (s *Server) broadcastEvents(ctx context.Context) {
	eventCh := s.kernelClient.Events()
	for {
		select {
		case <-ctx.Done():
			return
		case event, ok := <-eventCh:
			if !ok {
				return
			}
			s.broadcastWS(event)
		}
	}
}

// broadcastWS sends a message to all connected WebSocket clients.
// Memory cost: ~256 bytes per message per client
func (s *Server) broadcastWS(data interface{}) {
	s.wsMu.RLock()
	defer s.wsMu.RUnlock()

	for conn := range s.wsClients {
		if err := conn.WriteJSON(data); err != nil {
			log.Warn().Err(err).Msg("WebSocket write error")
		}
	}
}
