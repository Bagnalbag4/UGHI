// Package resilience implements circuit breaker and retry patterns for UGHI.
// Follows strict_rules.md | camelCase Go
// Memory cost: ~512 bytes per circuit breaker instance
// States: Closed (normal) → Open (failing) → HalfOpen (testing)
package resilience

import (
	"fmt"
	"sync"
	"time"

	"github.com/rs/zerolog/log"
)

// State represents the circuit breaker state.
type State int

const (
	StateClosed   State = iota // Normal operation
	StateOpen                  // Failing, reject all calls
	StateHalfOpen              // Testing if recovery is possible
)

func (s State) String() string {
	switch s {
	case StateClosed:
		return "closed"
	case StateOpen:
		return "open"
	case StateHalfOpen:
		return "half-open"
	default:
		return "unknown"
	}
}

// Config holds circuit breaker configuration.
// Memory cost: ~64 bytes
type Config struct {
	// FailureThreshold: number of failures before opening circuit.
	FailureThreshold int
	// CooldownDuration: time to wait before transitioning from Open → HalfOpen.
	CooldownDuration time.Duration
	// SuccessThreshold: successful calls in HalfOpen before closing circuit.
	SuccessThreshold int
	// Name for logging.
	Name string
}

// DefaultConfig returns sensible defaults.
func DefaultConfig(name string) Config {
	return Config{
		FailureThreshold: 5,
		CooldownDuration: 30 * time.Second,
		SuccessThreshold: 2,
		Name:             name,
	}
}

// CircuitBreaker implements the circuit breaker pattern.
// Memory cost: ~512 bytes
type CircuitBreaker struct {
	config          Config
	mu              sync.Mutex
	state           State
	failureCount    int
	successCount    int
	lastFailureTime time.Time
	totalFailures   int64
	totalSuccesses  int64
	totalRejected   int64
}

// NewCircuitBreaker creates a new circuit breaker.
// Memory cost: ~512 bytes
func NewCircuitBreaker(cfg Config) *CircuitBreaker {
	if cfg.FailureThreshold <= 0 {
		cfg.FailureThreshold = 5
	}
	if cfg.CooldownDuration <= 0 {
		cfg.CooldownDuration = 30 * time.Second
	}
	if cfg.SuccessThreshold <= 0 {
		cfg.SuccessThreshold = 2
	}

	return &CircuitBreaker{
		config: cfg,
		state:  StateClosed,
	}
}

// Execute runs the given function through the circuit breaker.
// Returns an error if the circuit is open and the function was not called.
func (cb *CircuitBreaker) Execute(fn func() error) error {
	cb.mu.Lock()

	switch cb.state {
	case StateOpen:
		// Check if cooldown has elapsed
		if time.Since(cb.lastFailureTime) >= cb.config.CooldownDuration {
			cb.state = StateHalfOpen
			cb.successCount = 0
			log.Info().
				Str("breaker", cb.config.Name).
				Msg("circuit breaker → half-open (cooldown elapsed)")
		} else {
			cb.totalRejected++
			cb.mu.Unlock()
			return fmt.Errorf("circuit breaker '%s' is open: rejecting call", cb.config.Name)
		}

	case StateHalfOpen:
		// Allow the call through for testing

	case StateClosed:
		// Normal operation
	}

	cb.mu.Unlock()

	// Execute the function
	err := fn()

	cb.mu.Lock()
	defer cb.mu.Unlock()

	if err != nil {
		cb.failureCount++
		cb.totalFailures++
		cb.lastFailureTime = time.Now()

		if cb.state == StateHalfOpen || cb.failureCount >= cb.config.FailureThreshold {
			cb.state = StateOpen
			log.Warn().
				Str("breaker", cb.config.Name).
				Int("failures", cb.failureCount).
				Msg("circuit breaker → open")
		}

		return err
	}

	// Success
	cb.totalSuccesses++

	switch cb.state {
	case StateHalfOpen:
		cb.successCount++
		if cb.successCount >= cb.config.SuccessThreshold {
			cb.state = StateClosed
			cb.failureCount = 0
			cb.successCount = 0
			log.Info().
				Str("breaker", cb.config.Name).
				Msg("circuit breaker → closed (recovered)")
		}
	case StateClosed:
		// Reset failure count on success (sliding window)
		cb.failureCount = 0
	}

	return nil
}

// State returns the current circuit breaker state.
func (cb *CircuitBreaker) CurrentState() State {
	cb.mu.Lock()
	defer cb.mu.Unlock()
	return cb.state
}

// Metrics returns circuit breaker metrics.
type Metrics struct {
	Name           string `json:"name"`
	State          string `json:"state"`
	FailureCount   int    `json:"failure_count"`
	TotalFailures  int64  `json:"total_failures"`
	TotalSuccesses int64  `json:"total_successes"`
	TotalRejected  int64  `json:"total_rejected"`
}

func (cb *CircuitBreaker) Metrics() Metrics {
	cb.mu.Lock()
	defer cb.mu.Unlock()
	return Metrics{
		Name:           cb.config.Name,
		State:          cb.state.String(),
		FailureCount:   cb.failureCount,
		TotalFailures:  cb.totalFailures,
		TotalSuccesses: cb.totalSuccesses,
		TotalRejected:  cb.totalRejected,
	}
}
