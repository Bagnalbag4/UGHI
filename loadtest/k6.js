import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate } from 'k6/metrics';

// Custom metrics
const successfulSpawns = new Counter('successful_spawns');
const spawnErrorRate = new Rate('spawn_errors');

export const options = {
    stages: [
        { duration: '30s', target: 50 },  // Ramp up to 50 users
        { duration: '1m', target: 50 },   // Stay at 50 users for 1 min
        { duration: '10s', target: 100 }, // Spike to 100 users
        { duration: '20s', target: 0 },   // Ramp down to 0
    ],
    thresholds: {
        http_req_duration: ['p(95)<200'], // 95% of requests must complete below 200ms
        spawn_errors: ['rate<0.01'],      // Less than 1% errors
    },
};

const BASE_URL = 'http://localhost:8080';
// In a real scenario, we would generate or pass a valid JWT token
const TOKEN = __ENV.UGHI_TEST_TOKEN || 'invalid-token-for-local-testing';

export default function () {
    const params = {
        headers: {
            'Authorization': `Bearer ${TOKEN}`,
            'Content-Type': 'application/json',
        },
    };

    // 1. Health check (no auth required)
    const healthRes = http.get(`${BASE_URL}/health`);
    check(healthRes, {
        'health ok': (r) => r.status === 200,
    });

    // 2. Spawn an agent (requires auth)
    const payload = JSON.stringify({
        goal: "Load test generated task " + Math.random(),
        priority: "normal"
    });

    const spawnRes = http.post(`${BASE_URL}/api/spawn`, payload, params);

    // Check if spawn was successful or if rate limited
    const isSuccessful = check(spawnRes, {
        'spawn successful (201)': (r) => r.status === 201,
        'rate limited (429)': (r) => r.status === 429,
    });

    if (spawnRes.status === 201) {
        successfulSpawns.add(1);
        spawnErrorRate.add(0);

        let agentId = spawnRes.json('agent_id');

        // Let the agent run a bit
        sleep(1);

        // 3. Monitor the agent
        const monitorRes = http.get(`${BASE_URL}/api/monitor/${agentId}`, params);
        check(monitorRes, {
            'monitor ok': (r) => r.status === 200,
        });

        // 4. Kill the agent to clean up
        const killRes = http.post(`${BASE_URL}/api/kill/${agentId}`, "{}", params);
        check(killRes, {
            'kill ok': (r) => r.status === 200,
        });
    } else if (spawnRes.status !== 429) {
        // Log unexpected errors (not a rate limit)
        spawnErrorRate.add(1);
        console.error(`Spawn failed with status ${spawnRes.status}: ${spawnRes.body}`);
    }

    sleep(0.5); // Think time between iterations
}
