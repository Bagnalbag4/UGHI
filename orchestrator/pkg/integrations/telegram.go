// Package integrations provides webhook adapters for chat platforms.
// Follows strict_rules.md | camelCase Go
// Memory cost: minimal (~2KB per request)
package integrations

import (
	"context"

	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog/log"
)

// TelegramWebhook handles incoming HTTP POST requests from the Telegram Bot API.
// POST /api/webhooks/telegram
func HandleTelegram(kernelClient *kernel.Client) fiber.Handler {
	return func(c *fiber.Ctx) error {
		var update struct {
			UpdateID int `json:"update_id"`
			Message  struct {
				MessageID int `json:"message_id"`
				Chat      struct {
					ID int64 `json:"id"`
				} `json:"chat"`
				Text string `json:"text"`
			} `json:"message"`
		}

		if err := c.BodyParser(&update); err != nil {
			log.Warn().Err(err).Msg("Failed to parse Telegram webhook")
			return c.SendStatus(fiber.StatusBadRequest)
		}

		// Only process text messages
		if update.Message.Text == "" {
			return c.SendStatus(fiber.StatusOK)
		}

		log.Info().
			Int64("chat_id", update.Message.Chat.ID).
			Str("text", update.Message.Text).
			Msg("Received Telegram message")

		// Route goal to kernel
		req := kernel.SpawnRequest{
			Goal:     update.Message.Text,
			Priority: "high", // Chat requests are usually high priority
		}

		ctx := context.Background()
		resp, err := kernelClient.Spawn(ctx, req)
		if err != nil {
			log.Error().Err(err).Msg("Failed to spawn agent from Telegram")
			// In a full implementation, we would HTTP POST back to Telegram API here
			// e.g. https://api.telegram.org/bot<token>/sendMessage
			return c.SendStatus(fiber.StatusInternalServerError)
		}

		log.Info().Str("agent_id", resp.AgentID).Msg("Spawned agent from Telegram webhook")

		// Always return 200 OK to Telegram so it stops retrying
		return c.SendStatus(fiber.StatusOK)
	}
}
