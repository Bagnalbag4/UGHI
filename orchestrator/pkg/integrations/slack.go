package integrations

import (
	"context"

	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog/log"
)

// HandleSlack processes Slack Events API webhooks (e.g. app_mention)
// POST /api/webhooks/slack
func HandleSlack(kernelClient *kernel.Client) fiber.Handler {
	return func(c *fiber.Ctx) error {
		var payload struct {
			Type      string `json:"type"`      // For URL verification
			Challenge string `json:"challenge"` // Challenge string for setup
			Event     struct {
				Type string `json:"type"`
				Text string `json:"text"`
				User string `json:"user"`
			} `json:"event"`
		}

		if err := c.BodyParser(&payload); err != nil {
			return c.SendStatus(fiber.StatusBadRequest)
		}

		// Handle Slack URL Verification handshake
		if payload.Type == "url_verification" {
			return c.JSON(fiber.Map{"challenge": payload.Challenge})
		}

		// Handle @mentions
		if payload.Event.Type == "app_mention" && payload.Event.Text != "" {
			goal := payload.Event.Text
			log.Info().Str("goal", goal).Str("user", payload.Event.User).Msg("Received Slack app_mention")

			req := kernel.SpawnRequest{Goal: goal, Priority: "high"}
			ctx := context.Background()
			resp, err := kernelClient.Spawn(ctx, req)
			if err != nil {
				log.Error().Err(err).Msg("Failed to spawn agent from Slack")
				return c.SendStatus(fiber.StatusInternalServerError)
			}

			log.Info().Str("agent_id", resp.AgentID).Msg("Spawned UGHI agent for Slack")
			// A full implementation would use the Slack chat.postMessage API here to reply
			// with the Agent ID and live progress updates.
		}

		return c.SendStatus(fiber.StatusOK)
	}
}
