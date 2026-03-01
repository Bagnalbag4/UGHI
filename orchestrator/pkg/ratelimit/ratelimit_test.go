package ratelimit

import (
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gofiber/fiber/v2"
)

func TestRateLimiter(t *testing.T) {
	cfg := Config{
		MaxRequests:    2,
		WindowDuration: 1 * time.Second,
		KeyFunc:        func(c *fiber.Ctx) string { return "test-client" },
		Enabled:        true,
	}
	limiter := New(cfg)

	// App with limitter middleware
	app := fiber.New()
	app.Use(limiter.Handler())
	app.Get("/", func(c *fiber.Ctx) error {
		return c.SendString("ok")
	})

	// Request 1: Allowed
	req1 := httptest.NewRequest("GET", "/", nil)
	resp1, _ := app.Test(req1)
	if resp1.StatusCode == 429 {
		t.Errorf("expected request 1 to be allowed, got %d", resp1.StatusCode)
	}

	// Request 2: Allowed
	req2 := httptest.NewRequest("GET", "/", nil)
	resp2, _ := app.Test(req2)
	if resp2.StatusCode == 429 {
		t.Errorf("expected request 2 to be allowed, got %d", resp2.StatusCode)
	}

	// Request 3: Rate Limited
	req3 := httptest.NewRequest("GET", "/", nil)
	resp3, _ := app.Test(req3)
	if resp3.StatusCode != 429 {
		t.Errorf("expected request 3 to be rate limited (429), got %d", resp3.StatusCode)
	}
}
