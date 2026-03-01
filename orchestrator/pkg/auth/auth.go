// Package auth implements JWT-based authentication middleware for UGHI.
// Follows strict_rules.md | camelCase Go | Zero trust
// Memory cost: ~2 KB per active token validation
// Supports HS256 signing with configurable secret.
// Roles: admin (full access), operator (spawn/kill/manage), viewer (read-only)
package auth

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog/log"
)

// Role defines authorization levels.
// Memory cost: ~16 bytes (string)
type Role string

const (
	RoleAdmin    Role = "admin"
	RoleOperator Role = "operator"
	RoleViewer   Role = "viewer"
)

// Claims represents JWT claims.
// Memory cost: ~128 bytes
type Claims struct {
	Sub  string `json:"sub"`
	Role Role   `json:"role"`
	Iat  int64  `json:"iat"`
	Exp  int64  `json:"exp"`
}

// Config holds auth middleware configuration.
// Memory cost: ~256 bytes
type Config struct {
	// Secret for HS256 signing. Falls back to UGHI_JWT_SECRET env var.
	Secret string
	// SkipPaths are paths that don't require authentication.
	SkipPaths []string
	// Enabled controls whether auth is active. Defaults to true if secret is set.
	Enabled bool
}

// Middleware holds the JWT validation state.
// Memory cost: ~1 KB
type Middleware struct {
	secret    []byte
	skipPaths map[string]bool
	enabled   bool
	// Track revoked tokens (in production: use Redis)
	revoked   map[string]int64
	revokedMu sync.RWMutex
}

// New creates a new auth middleware.
// Memory cost: ~1 KB
func New(cfg Config) *Middleware {
	secret := cfg.Secret
	if secret == "" {
		secret = os.Getenv("UGHI_JWT_SECRET")
	}

	enabled := cfg.Enabled
	if secret == "" {
		enabled = false
		log.Warn().Msg("JWT auth disabled: no UGHI_JWT_SECRET set")
	}

	skipPaths := make(map[string]bool, len(cfg.SkipPaths))
	for _, p := range cfg.SkipPaths {
		skipPaths[p] = true
	}

	return &Middleware{
		secret:    []byte(secret),
		skipPaths: skipPaths,
		enabled:   enabled,
		revoked:   make(map[string]int64),
	}
}

// Handler returns a Fiber middleware handler that validates JWT tokens.
// Memory cost: ~256 bytes per request
func (m *Middleware) Handler() fiber.Handler {
	return func(c *fiber.Ctx) error {
		if !m.enabled {
			return c.Next()
		}

		// Skip configured paths
		path := c.Path()
		if m.skipPaths[path] {
			return c.Next()
		}
		// Skip paths with prefix match
		for p := range m.skipPaths {
			if strings.HasSuffix(p, "*") && strings.HasPrefix(path, strings.TrimSuffix(p, "*")) {
				return c.Next()
			}
		}

		// Extract token from Authorization header
		authHeader := c.Get("Authorization")
		if authHeader == "" {
			return c.Status(401).JSON(fiber.Map{
				"error":   "unauthorized",
				"message": "missing Authorization header",
			})
		}

		if !strings.HasPrefix(authHeader, "Bearer ") {
			return c.Status(401).JSON(fiber.Map{
				"error":   "unauthorized",
				"message": "invalid Authorization format, expected: Bearer <token>",
			})
		}

		token := strings.TrimPrefix(authHeader, "Bearer ")
		claims, err := m.ValidateToken(token)
		if err != nil {
			return c.Status(401).JSON(fiber.Map{
				"error":   "unauthorized",
				"message": err.Error(),
			})
		}

		// Store claims in context for downstream handlers
		c.Locals("claims", claims)
		c.Locals("role", string(claims.Role))
		c.Locals("sub", claims.Sub)

		return c.Next()
	}
}

// RequireRole returns a middleware that checks for minimum role level.
// Memory cost: ~64 bytes
func RequireRole(minRole Role) fiber.Handler {
	return func(c *fiber.Ctx) error {
		role, ok := c.Locals("role").(string)
		if !ok {
			return c.Status(403).JSON(fiber.Map{"error": "forbidden", "message": "no role in context"})
		}

		if !hasPermission(Role(role), minRole) {
			return c.Status(403).JSON(fiber.Map{
				"error":   "forbidden",
				"message": fmt.Sprintf("requires role '%s', got '%s'", minRole, role),
			})
		}

		return c.Next()
	}
}

// GenerateToken creates a new JWT token.
// Memory cost: ~512 bytes
func (m *Middleware) GenerateToken(sub string, role Role, ttl time.Duration) (string, error) {
	if len(m.secret) == 0 {
		return "", fmt.Errorf("no signing secret configured")
	}

	now := time.Now().Unix()
	claims := Claims{
		Sub:  sub,
		Role: role,
		Iat:  now,
		Exp:  now + int64(ttl.Seconds()),
	}

	// Header
	header := base64URLEncode([]byte(`{"alg":"HS256","typ":"JWT"}`))

	// Payload
	payloadBytes, err := json.Marshal(claims)
	if err != nil {
		return "", fmt.Errorf("failed to marshal claims: %w", err)
	}
	payload := base64URLEncode(payloadBytes)

	// Signature
	sigInput := header + "." + payload
	sig := hmacSha256([]byte(sigInput), m.secret)

	return sigInput + "." + base64URLEncode(sig), nil
}

// ValidateToken verifies a JWT token and returns claims.
// Memory cost: ~256 bytes
func (m *Middleware) ValidateToken(token string) (*Claims, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return nil, fmt.Errorf("invalid token format")
	}

	// Check revocation
	m.revokedMu.RLock()
	if _, revoked := m.revoked[token]; revoked {
		m.revokedMu.RUnlock()
		return nil, fmt.Errorf("token has been revoked")
	}
	m.revokedMu.RUnlock()

	// Verify signature
	sigInput := parts[0] + "." + parts[1]
	expectedSig := hmacSha256([]byte(sigInput), m.secret)
	actualSig, err := base64URLDecode(parts[2])
	if err != nil {
		return nil, fmt.Errorf("invalid signature encoding")
	}
	if !hmac.Equal(expectedSig, actualSig) {
		return nil, fmt.Errorf("invalid signature")
	}

	// Decode claims
	claimsBytes, err := base64URLDecode(parts[1])
	if err != nil {
		return nil, fmt.Errorf("invalid payload encoding")
	}

	var claims Claims
	if err := json.Unmarshal(claimsBytes, &claims); err != nil {
		return nil, fmt.Errorf("invalid claims: %w", err)
	}

	// Check expiry
	if claims.Exp > 0 && time.Now().Unix() > claims.Exp {
		return nil, fmt.Errorf("token expired")
	}

	return &claims, nil
}

// RevokeToken marks a token as revoked.
// Memory cost: ~64 bytes per revoked token
func (m *Middleware) RevokeToken(token string) {
	m.revokedMu.Lock()
	m.revoked[token] = time.Now().Unix()
	m.revokedMu.Unlock()
}

// CleanupExpiredRevocations removes old revocation entries.
func (m *Middleware) CleanupExpiredRevocations(maxAge time.Duration) {
	m.revokedMu.Lock()
	defer m.revokedMu.Unlock()
	cutoff := time.Now().Add(-maxAge).Unix()
	for token, revokedAt := range m.revoked {
		if revokedAt < cutoff {
			delete(m.revoked, token)
		}
	}
}

// --- Helpers ---

func hasPermission(actual, required Role) bool {
	levels := map[Role]int{
		RoleViewer:   1,
		RoleOperator: 2,
		RoleAdmin:    3,
	}
	return levels[actual] >= levels[required]
}

func hmacSha256(data, secret []byte) []byte {
	h := hmac.New(sha256.New, secret)
	h.Write(data)
	return h.Sum(nil)
}

func base64URLEncode(data []byte) string {
	return base64.RawURLEncoding.EncodeToString(data)
}

func base64URLDecode(s string) ([]byte, error) {
	return base64.RawURLEncoding.DecodeString(s)
}
