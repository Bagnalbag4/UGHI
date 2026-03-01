// handlers.go – REST + WebSocket route handlers for Agenticos API.
// Follows strict_rules.md | camelCase Go
// Memory cost: minimal per request (~1 KB response buffers)
// All handlers return JSON. No blocking calls.
package api

import (
	"context"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/websocket/v2"
	"github.com/rs/zerolog/log"

	"github.com/agenticos/orchestrator/pkg/dashboard"
	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/agenticos/orchestrator/pkg/supervisor"
)

// --- REST Handlers ---

// handleHealth returns system health status.
func (s *Server) handleHealth(c *fiber.Ctx) error {
	return c.JSON(fiber.Map{
		"status":    "ok",
		"version":   "0.1.0",
		"connected": s.kernelClient.IsConnected(),
		"agents":    s.kernelClient.AgentCount(),
		"uptime":    time.Now().Unix(),
	})
}

// handleListAgents returns all agent snapshots.
// GET /api/agents
func (s *Server) handleListAgents(c *fiber.Ctx) error {
	ctx := context.Background()
	agents := s.kernelClient.ListAgents(ctx)
	return c.JSON(fiber.Map{
		"agents": agents,
		"count":  len(agents),
	})
}

// handleSpawn creates a new agent.
// POST /api/spawn { "goal": "...", "priority": "...", "parent_id": "..." }
func (s *Server) handleSpawn(c *fiber.Ctx) error {
	var req kernel.SpawnRequest
	if err := c.BodyParser(&req); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "invalid request body"})
	}

	if req.Goal == "" {
		return c.Status(400).JSON(fiber.Map{"error": "goal is required"})
	}

	ctx := context.Background()
	resp, err := s.kernelClient.Spawn(ctx, req)
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}

	// Add to supervisor tree
	s.supervisorTree.AddNode(resp.AgentID, req.ParentID, req.Goal, req.Priority)

	return c.Status(201).JSON(fiber.Map{
		"agent_id":         resp.AgentID,
		"spawn_latency_us": resp.SpawnLatencyUs,
		"goal":             req.Goal,
		"state":            "spawned",
	})
}

// handleKill removes an agent.
// POST /api/kill/:id
func (s *Server) handleKill(c *fiber.Ctx) error {
	agentID := c.Params("id")
	if agentID == "" {
		return c.Status(400).JSON(fiber.Map{"error": "agent_id is required"})
	}

	ctx := context.Background()
	snapshot, err := s.kernelClient.Kill(ctx, agentID)
	if err != nil {
		return c.Status(404).JSON(fiber.Map{"error": err.Error()})
	}

	// Remove from supervisor tree
	s.supervisorTree.RemoveNode(agentID)

	return c.JSON(fiber.Map{
		"killed":   true,
		"snapshot": snapshot,
	})
}

// handleMonitor returns a specific agent's state.
// GET /api/monitor/:id
func (s *Server) handleMonitor(c *fiber.Ctx) error {
	agentID := c.Params("id")
	if agentID == "" {
		return c.Status(400).JSON(fiber.Map{"error": "agent_id is required"})
	}

	ctx := context.Background()
	snapshot, err := s.kernelClient.Monitor(ctx, agentID)
	if err != nil {
		return c.Status(404).JSON(fiber.Map{"error": err.Error()})
	}

	return c.JSON(snapshot)
}

// handleTransition changes an agent's lifecycle state.
// POST /api/transition { "agent_id": "...", "new_state": "..." }
func (s *Server) handleTransition(c *fiber.Ctx) error {
	var req kernel.TransitionRequest
	if err := c.BodyParser(&req); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "invalid request body"})
	}

	ctx := context.Background()
	if err := s.kernelClient.Transition(ctx, req); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": err.Error()})
	}

	// Update supervisor tree state
	s.supervisorTree.UpdateState(req.AgentID, supervisor.NodeState(req.NewState))

	return c.JSON(fiber.Map{
		"agent_id":  req.AgentID,
		"new_state": req.NewState,
	})
}

// handleMetrics returns runtime metrics.
// GET /api/metrics
func (s *Server) handleMetrics(c *fiber.Ctx) error {
	ctx := context.Background()
	metrics := s.kernelClient.GetMetrics(ctx)
	return c.JSON(metrics)
}

// handleTree returns the supervisor tree JSON.
// GET /api/tree
func (s *Server) handleTree(c *fiber.Ctx) error {
	data, err := s.supervisorTree.Serialize()
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}
	c.Set("Content-Type", "application/json")
	return c.Send(data)
}

// handleRunAgent spawns and runs a one-shot agent.
// POST /api/run { "goal": "...", "priority": "..." }
func (s *Server) handleRunAgent(c *fiber.Ctx) error {
	var req struct {
		Goal     string `json:"goal"`
		Priority string `json:"priority"`
	}
	if err := c.BodyParser(&req); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "invalid request body"})
	}

	if req.Goal == "" {
		return c.Status(400).JSON(fiber.Map{"error": "goal is required"})
	}

	if req.Priority == "" {
		req.Priority = "high"
	}

	ctx := context.Background()
	snapshot, err := s.kernelClient.RunAgent(ctx, req.Goal, req.Priority)
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}

	return c.JSON(fiber.Map{
		"result":   snapshot,
		"complete": true,
	})
}

// --- Dashboard Handler ---

// handleDashboard serves the embedded HTML dashboard.
// GET /dashboard
func (s *Server) handleDashboard(c *fiber.Ctx) error {
	c.Set("Content-Type", "text/html; charset=utf-8")
	return c.SendString(dashboard.DashboardHTML)
}

// --- WebSocket Handler ---

// handleWebSocket handles WebSocket connections for real-time updates.
// Memory cost: ~4 KB per connection
func (s *Server) handleWebSocket(c *websocket.Conn) {
	// Register client
	s.wsMu.Lock()
	s.wsClients[c] = true
	clientCount := len(s.wsClients)
	s.wsMu.Unlock()

	log.Info().
		Int("clients", clientCount).
		Str("addr", c.RemoteAddr().String()).
		Msg("WebSocket client connected")

	// Send initial state
	if data, err := s.kernelClient.Serialize(); err == nil {
		_ = c.WriteMessage(websocket.TextMessage, data)
	}

	// Subscribe to tree events
	treeCh := s.supervisorTree.Subscribe()
	defer s.supervisorTree.Unsubscribe(treeCh)

	// Forward tree events to this client
	go func() {
		for event := range treeCh {
			if err := c.WriteJSON(event); err != nil {
				return
			}
		}
	}()

	// Read loop (keeps connection alive, handles close)
	for {
		_, _, err := c.ReadMessage()
		if err != nil {
			break
		}
	}

	// Unregister client
	s.wsMu.Lock()
	delete(s.wsClients, c)
	s.wsMu.Unlock()

	log.Info().Str("addr", c.RemoteAddr().String()).Msg("WebSocket client disconnected")
}
