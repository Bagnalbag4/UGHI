package integrations

import (
	"context"

	"github.com/agenticos/orchestrator/pkg/kernel"
	"github.com/gofiber/fiber/v2"
	"github.com/rs/zerolog/log"
)

// HandleDiscord handles incoming HTTP POST requests from Discord Interactions.
// POST /api/webhooks/discord
func HandleDiscord(kernelClient *kernel.Client) fiber.Handler {
	return func(c *fiber.Ctx) error {
		// Discord expects a specific security handshake (Ping-Pong) using Ed25519 signature validation
		var interaction struct {
			Type int `json:"type"` // 1 = PING, 2 = APPLICATION_COMMAND
			Data struct {
				Name    string `json:"name"`
				Options []struct {
					Name  string      `json:"name"`
					Value interface{} `json:"value"`
				} `json:"options"`
			} `json:"data"`
		}

		if err := c.BodyParser(&interaction); err != nil {
			return c.SendStatus(fiber.StatusBadRequest)
		}

		// Handle Discord PING validation
		if interaction.Type == 1 {
			return c.JSON(fiber.Map{"type": 1})
		}

		// Handle Discord Slash Command (e.g. /ughi <goal>)
		if interaction.Type == 2 && interaction.Data.Name == "ughi" {
			var goal string
			for _, opt := range interaction.Data.Options {
				if opt.Name == "goal" {
					if v, ok := opt.Value.(string); ok {
						goal = v
					}
				}
			}

			if goal != "" {
				log.Info().Str("goal", goal).Msg("Received Discord slash command")

				req := kernel.SpawnRequest{Goal: goal, Priority: "high"}
				ctx := context.Background()
				resp, err := kernelClient.Spawn(ctx, req)
				if err != nil {
					log.Error().Err(err).Msg("Failed to spawn agent from Discord")
					return c.JSON(fiber.Map{
						"type": 4, // CHANNEL_MESSAGE_WITH_SOURCE
						"data": fiber.Map{"content": "Error spawning UGHI agent: " + err.Error()},
					})
				}

				return c.JSON(fiber.Map{
					"type": 4,
					"data": fiber.Map{"content": "UGHI Agent spawned! ID: " + resp.AgentID},
				})
			}
		}

		return c.SendStatus(fiber.StatusOK)
	}
}
