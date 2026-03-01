// Package ratelimit implements a token bucket rate limiter for UGHI API.
// Follows strict_rules.md | camelCase Go | Zero trust
// Memory cost: ~256 bytes per tracked client + ~1 KB for middleware state
// Configurable per-endpoint limits. Defaults: 100 req/min for reads, 20 req/min for writes.
package ratelimit

import (
	"sync"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog/log"
)

// Config holds rate limiter configuration.
// Memory cost: ~128 bytes
type Config struct {
	// MaxRequests per window period. Default: 100
	MaxRequests int
	// WindowDuration for the rate limit window. Default: 1 minute
	WindowDuration time.Duration
	// KeyFunc extracts the client identifier from the request.
	// Default: uses remote IP address.
	KeyFunc func(c *fiber.Ctx) string
	// SkipPaths are paths that bypass rate limiting.
	SkipPaths []string
	// Enabled controls whether rate limiting is active. Default: true.
	Enabled bool
}

// DefaultConfig returns sensible defaults.
func DefaultConfig() Config {
	return Config{
		MaxRequests:    100,
		WindowDuration: 1 * time.Minute,
		KeyFunc:        func(c *fiber.Ctx) string { return c.IP() },
		Enabled:        true,
	}
}

// WriteConfig returns stricter limits for write endpoints.
func WriteConfig() Config {
	return Config{
		MaxRequests:    20,
		WindowDuration: 1 * time.Minute,
		KeyFunc:        func(c *fiber.Ctx) string { return c.IP() },
		Enabled:        true,
	}
}

// entry tracks request count per window for a client.
// Memory cost: ~32 bytes
type entry struct {
	count    int
	windowAt time.Time
}

// Limiter implements a sliding window rate limiter.
// Memory cost: ~256 bytes per tracked client
type Limiter struct {
	config  Config
	entries map[string]*entry
	mu      sync.Mutex
	skip    map[string]bool
}

// New creates a new rate limiter.
// Memory cost: ~1 KB base
func New(cfg Config) *Limiter {
	if cfg.MaxRequests <= 0 {
		cfg.MaxRequests = 100
	}
	if cfg.WindowDuration <= 0 {
		cfg.WindowDuration = 1 * time.Minute
	}
	if cfg.KeyFunc == nil {
		cfg.KeyFunc = func(c *fiber.Ctx) string { return c.IP() }
	}

	skip := make(map[string]bool, len(cfg.SkipPaths))
	for _, p := range cfg.SkipPaths {
		skip[p] = true
	}

	l := &Limiter{
		config:  cfg,
		entries: make(map[string]*entry, 256),
		skip:    skip,
	}

	// Background cleanup every window duration
	go l.cleanup()

	return l
}

// Handler returns a Fiber middleware that enforces rate limits.
// Memory cost: ~64 bytes per request
func (l *Limiter) Handler() fiber.Handler {
	return func(c *fiber.Ctx) error {
		if !l.config.Enabled {
			return c.Next()
		}

		path := c.Path()
		if l.skip[path] {
			return c.Next()
		}

		key := l.config.KeyFunc(c)
		allowed, remaining, resetAt := l.allow(key)

		// Set rate limit headers (RFC 6585 / draft-polli-ratelimit-headers)
		c.Set("X-RateLimit-Limit", itoa(l.config.MaxRequests))
		c.Set("X-RateLimit-Remaining", itoa(remaining))
		c.Set("X-RateLimit-Reset", itoa(int(resetAt.Unix())))

		if !allowed {
			retryAfter := int(time.Until(resetAt).Seconds()) + 1
			c.Set("Retry-After", itoa(retryAfter))

			log.Warn().
				Str("client", key).
				Str("path", path).
				Int("limit", l.config.MaxRequests).
				Msg("rate limit exceeded")

			return c.Status(429).JSON(fiber.Map{
				"error":       "too_many_requests",
				"message":     "rate limit exceeded",
				"retry_after": retryAfter,
			})
		}

		return c.Next()
	}
}

// allow checks if a request is allowed and returns remaining count + reset time.
func (l *Limiter) allow(key string) (bool, int, time.Time) {
	l.mu.Lock()
	defer l.mu.Unlock()

	now := time.Now()
	e, exists := l.entries[key]
	if !exists || now.After(e.windowAt) {
		// New window
		l.entries[key] = &entry{
			count:    1,
			windowAt: now.Add(l.config.WindowDuration),
		}
		return true, l.config.MaxRequests - 1, now.Add(l.config.WindowDuration)
	}

	e.count++
	remaining := l.config.MaxRequests - e.count
	if remaining < 0 {
		remaining = 0
	}

	return e.count <= l.config.MaxRequests, remaining, e.windowAt
}

// cleanup runs periodically to remove expired entries.
func (l *Limiter) cleanup() {
	ticker := time.NewTicker(l.config.WindowDuration)
	defer ticker.Stop()
	for range ticker.C {
		l.mu.Lock()
		now := time.Now()
		for key, e := range l.entries {
			if now.After(e.windowAt) {
				delete(l.entries, key)
			}
		}
		l.mu.Unlock()
	}
}

// Metrics returns current rate limiter state.
type Metrics struct {
	TrackedClients int           `json:"tracked_clients"`
	Config         MetricsConfig `json:"config"`
}

type MetricsConfig struct {
	MaxRequests   int `json:"max_requests"`
	WindowSeconds int `json:"window_seconds"`
}

func (l *Limiter) Metrics() Metrics {
	l.mu.Lock()
	count := len(l.entries)
	l.mu.Unlock()
	return Metrics{
		TrackedClients: count,
		Config: MetricsConfig{
			MaxRequests:   l.config.MaxRequests,
			WindowSeconds: int(l.config.WindowDuration.Seconds()),
		},
	}
}

func itoa(n int) string {
	// Simple int to string without importing strconv
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	buf := make([]byte, 0, 12)
	for n > 0 {
		buf = append(buf, byte('0'+n%10))
		n /= 10
	}
	if neg {
		buf = append(buf, '-')
	}
	// Reverse
	for i, j := 0, len(buf)-1; i < j; i, j = i+1, j-1 {
		buf[i], buf[j] = buf[j], buf[i]
	}
	return string(buf)
}
