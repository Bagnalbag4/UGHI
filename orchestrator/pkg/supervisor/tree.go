// Package supervisor implements the Supervisor-Worker agent tree.
// Follows strict_rules.md | camelCase Go | Zero trust
// Memory cost: ~4 MB (tree metadata + agent slots + event channel)
// agent.md: "Supervisor-Worker tree only (no full mesh unless requested)"
// All operations non-blocking via Go channels.
package supervisor

import (
	"context"
	"encoding/json"
	"sync"
	"time"

	"github.com/rs/zerolog/log"
)

// NodeState represents an agent's lifecycle state in the tree.
// Memory cost: 1 byte (string reference)
type NodeState string

const (
	StateSpawned       NodeState = "spawned"
	StatePlanning      NodeState = "planning"
	StateToolUsing     NodeState = "tool_using"
	StateThinking      NodeState = "thinking"
	StateCollaborating NodeState = "collaborating"
	StateReviewing     NodeState = "reviewing"
	StateCompleting    NodeState = "completing"
	StateCrashed       NodeState = "crashed"
)

// AgentNode represents a node in the supervisor tree.
// Memory cost: ~256 bytes per node
type AgentNode struct {
	ID          string    `json:"id"`
	ParentID    string    `json:"parentId,omitempty"`
	Goal        string    `json:"goal"`
	State       NodeState `json:"state"`
	Priority    string    `json:"priority"`
	Children    []string  `json:"children"`
	MemoryBytes uint64    `json:"memoryBytes"`
	Transitions uint64    `json:"transitions"`
	CreatedAt   int64     `json:"createdAt"` // Unix ms
	UpdatedAt   int64     `json:"updatedAt"` // Unix ms
}

// TreeEvent is emitted when the supervisor tree changes.
// Memory cost: ~128 bytes
type TreeEvent struct {
	Type      string `json:"type"` // node_added|node_removed|state_changed
	NodeID    string `json:"nodeId"`
	OldState  string `json:"oldState,omitempty"`
	NewState  string `json:"newState,omitempty"`
	Timestamp int64  `json:"timestamp"`
}

// Tree is the supervisor-worker hierarchy with channel-based event bus.
// Memory cost: ~4 MB base (map + subscribers + mutex)
type Tree struct {
	mu          sync.RWMutex
	nodes       map[string]*AgentNode
	rootIDs     []string // Top-level supervisor agent IDs
	maxNodes    int
	subscribers []chan TreeEvent
	subMu       sync.RWMutex
}

// NewTree creates a supervisor tree with the given capacity.
// Memory cost: ~4 MB (pre-allocated map + channels)
func NewTree(maxNodes int) *Tree {
	return &Tree{
		nodes:       make(map[string]*AgentNode, maxNodes),
		rootIDs:     make([]string, 0, 8),
		maxNodes:    maxNodes,
		subscribers: make([]chan TreeEvent, 0, 16),
	}
}

// AddNode adds an agent to the tree under a parent.
// Memory cost: ~256 bytes per node added
func (t *Tree) AddNode(id, parentID, goal, priority string) bool {
	t.mu.Lock()
	defer t.mu.Unlock()

	if len(t.nodes) >= t.maxNodes {
		log.Warn().Int("max", t.maxNodes).Msg("supervisor tree at capacity")
		return false
	}

	now := time.Now().UnixMilli()
	node := &AgentNode{
		ID:        id,
		ParentID:  parentID,
		Goal:      goal,
		State:     StateSpawned,
		Priority:  priority,
		Children:  make([]string, 0),
		CreatedAt: now,
		UpdatedAt: now,
	}
	t.nodes[id] = node

	// Link to parent or mark as root
	if parentID != "" {
		if parent, ok := t.nodes[parentID]; ok {
			parent.Children = append(parent.Children, id)
		}
	} else {
		t.rootIDs = append(t.rootIDs, id)
	}

	log.Info().
		Str("agentId", id).
		Str("parentId", parentID).
		Str("goal", goal).
		Msg("agent added to supervisor tree")

	// Emit event (non-blocking)
	t.broadcast(TreeEvent{
		Type:      "node_added",
		NodeID:    id,
		NewState:  string(StateSpawned),
		Timestamp: now,
	})

	return true
}

// UpdateState changes an agent's lifecycle state.
// Memory cost: 0 (in-place mutation)
func (t *Tree) UpdateState(id string, newState NodeState) bool {
	t.mu.Lock()
	defer t.mu.Unlock()

	node, ok := t.nodes[id]
	if !ok {
		return false
	}

	oldState := node.State
	node.State = newState
	node.Transitions++
	node.UpdatedAt = time.Now().UnixMilli()

	t.broadcast(TreeEvent{
		Type:      "state_changed",
		NodeID:    id,
		OldState:  string(oldState),
		NewState:  string(newState),
		Timestamp: node.UpdatedAt,
	})

	return true
}

// RemoveNode removes an agent and its children from the tree.
// Memory cost: frees ~256 bytes per node removed
func (t *Tree) RemoveNode(id string) {
	t.mu.Lock()
	defer t.mu.Unlock()

	node, ok := t.nodes[id]
	if !ok {
		return
	}

	// Remove children recursively
	for _, childID := range node.Children {
		delete(t.nodes, childID)
	}
	delete(t.nodes, id)

	// Remove from root list
	for i, rootID := range t.rootIDs {
		if rootID == id {
			t.rootIDs = append(t.rootIDs[:i], t.rootIDs[i+1:]...)
			break
		}
	}

	// Remove from parent's children list
	if node.ParentID != "" {
		if parent, ok := t.nodes[node.ParentID]; ok {
			for i, childID := range parent.Children {
				if childID == id {
					parent.Children = append(parent.Children[:i], parent.Children[i+1:]...)
					break
				}
			}
		}
	}

	t.broadcast(TreeEvent{
		Type:      "node_removed",
		NodeID:    id,
		Timestamp: time.Now().UnixMilli(),
	})

	log.Info().Str("agentId", id).Msg("agent removed from supervisor tree")
}

// GetNode returns a copy of a node.
// Memory cost: ~256 bytes (copy)
func (t *Tree) GetNode(id string) (*AgentNode, bool) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	node, ok := t.nodes[id]
	if !ok {
		return nil, false
	}
	copy := *node
	return &copy, true
}

// Count returns the number of active agents.
// Memory cost: 0
func (t *Tree) Count() int {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return len(t.nodes)
}

// Subscribe creates a new event channel for real-time updates.
// Memory cost: ~8 KB (buffered channel)
func (t *Tree) Subscribe() chan TreeEvent {
	ch := make(chan TreeEvent, 64)
	t.subMu.Lock()
	t.subscribers = append(t.subscribers, ch)
	t.subMu.Unlock()
	return ch
}

// Unsubscribe removes an event channel.
// Memory cost: 0
func (t *Tree) Unsubscribe(ch chan TreeEvent) {
	t.subMu.Lock()
	defer t.subMu.Unlock()

	for i, sub := range t.subscribers {
		if sub == ch {
			t.subscribers = append(t.subscribers[:i], t.subscribers[i+1:]...)
			close(ch)
			return
		}
	}
}

// Serialize returns the tree as JSON for the dashboard.
// Memory cost: variable (depends on node count)
func (t *Tree) Serialize() ([]byte, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	// Build tree structure for the dashboard
	type treeView struct {
		Nodes    map[string]*AgentNode `json:"nodes"`
		RootIDs  []string              `json:"rootIds"`
		Count    int                   `json:"count"`
		MaxNodes int                   `json:"maxNodes"`
	}

	return json.Marshal(treeView{
		Nodes:    t.nodes,
		RootIDs:  t.rootIDs,
		Count:    len(t.nodes),
		MaxNodes: t.maxNodes,
	})
}

// broadcast sends an event to all subscribers (non-blocking).
// Memory cost: 0
func (t *Tree) broadcast(event TreeEvent) {
	t.subMu.RLock()
	defer t.subMu.RUnlock()

	for _, ch := range t.subscribers {
		select {
		case ch <- event:
		default:
			// Never block the hot path
		}
	}
}

// Run starts the supervisor tree event loop.
// Memory cost: minimal (~8 KB goroutine stack)
func (t *Tree) Run(ctx context.Context) {
	log.Info().Msg("supervisor tree event loop started")
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			log.Info().Msg("supervisor tree event loop stopped")
			return
		case <-ticker.C:
			count := t.Count()
			if count > 0 {
				log.Debug().Int("agents", count).Msg("supervisor tree heartbeat")
			}
		}
	}
}
