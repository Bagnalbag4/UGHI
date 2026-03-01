// Package metrics implements Prometheus-compatible metrics export for UGHI.
// Follows strict_rules.md | camelCase Go
// Memory cost: ~4 KB (counters + gauges + histograms)
// Endpoint: GET /metrics (Prometheus text format)
package metrics

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/gofiber/fiber/v2"
)

// Collector holds all UGHI metrics.
// Memory cost: ~4 KB
type Collector struct {
	// Counters
	requestsTotal    atomic.Int64
	requestsByMethod sync.Map // method -> *atomic.Int64
	requestsByStatus sync.Map // status_code -> *atomic.Int64
	agentSpawnsTotal atomic.Int64
	agentKillsTotal  atomic.Int64
	skillExecsTotal  atomic.Int64
	errorsTotal      atomic.Int64

	// Gauges
	agentsActive     atomic.Int64
	agentsHibernated atomic.Int64
	memoryUsedBytes  atomic.Int64
	goroutineCount   atomic.Int64
	wsClientsActive  atomic.Int64

	// Histograms (simplified: fixed buckets, no external deps)
	requestLatencies *Histogram
	agentLifecycleMs *Histogram

	// Boot time
	bootTime time.Time
}

// Histogram is a simple fixed-bucket histogram.
// Memory cost: ~256 bytes
type Histogram struct {
	mu      sync.Mutex
	buckets []float64
	counts  []int64
	sum     float64
	count   int64
}

// NewHistogram creates a histogram with the given bucket boundaries.
func NewHistogram(buckets []float64) *Histogram {
	return &Histogram{
		buckets: buckets,
		counts:  make([]int64, len(buckets)+1), // +1 for +Inf
	}
}

// Observe records a value.
func (h *Histogram) Observe(value float64) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.sum += value
	h.count++
	for i, b := range h.buckets {
		if value <= b {
			h.counts[i]++
			return
		}
	}
	h.counts[len(h.buckets)]++ // +Inf bucket
}

// New creates a new metrics collector.
// Memory cost: ~4 KB
func New() *Collector {
	return &Collector{
		requestLatencies: NewHistogram([]float64{
			1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000,
		}),
		agentLifecycleMs: NewHistogram([]float64{
			10, 50, 100, 500, 1000, 5000, 10000, 30000,
		}),
		bootTime: time.Now(),
	}
}

// --- Increment methods ---

func (c *Collector) IncRequests(method string, status int) {
	c.requestsTotal.Add(1)

	// By method
	val, _ := c.requestsByMethod.LoadOrStore(method, &atomic.Int64{})
	val.(*atomic.Int64).Add(1)

	// By status
	statusKey := fmt.Sprintf("%d", status)
	sval, _ := c.requestsByStatus.LoadOrStore(statusKey, &atomic.Int64{})
	sval.(*atomic.Int64).Add(1)
}

func (c *Collector) IncAgentSpawns() { c.agentSpawnsTotal.Add(1) }
func (c *Collector) IncAgentKills()  { c.agentKillsTotal.Add(1) }
func (c *Collector) IncSkillExecs()  { c.skillExecsTotal.Add(1) }
func (c *Collector) IncErrors()      { c.errorsTotal.Add(1) }

func (c *Collector) SetAgentsActive(n int64)     { c.agentsActive.Store(n) }
func (c *Collector) SetAgentsHibernated(n int64) { c.agentsHibernated.Store(n) }
func (c *Collector) SetMemoryUsedBytes(n int64)  { c.memoryUsedBytes.Store(n) }
func (c *Collector) SetGoroutineCount(n int64)   { c.goroutineCount.Store(n) }
func (c *Collector) SetWSClients(n int64)        { c.wsClientsActive.Store(n) }

func (c *Collector) ObserveRequestLatency(ms float64) { c.requestLatencies.Observe(ms) }
func (c *Collector) ObserveAgentLifecycle(ms float64) { c.agentLifecycleMs.Observe(ms) }

// Handler returns a Fiber handler for GET /metrics (Prometheus text format).
func (c *Collector) Handler() fiber.Handler {
	return func(ctx *fiber.Ctx) error {
		ctx.Set("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
		return ctx.SendString(c.Render())
	}
}

// Render outputs all metrics in Prometheus text exposition format.
func (c *Collector) Render() string {
	var b strings.Builder

	// Counters
	writeCounter(&b, "ughi_requests_total", "Total HTTP requests", c.requestsTotal.Load())
	writeCounter(&b, "ughi_agent_spawns_total", "Total agents spawned", c.agentSpawnsTotal.Load())
	writeCounter(&b, "ughi_agent_kills_total", "Total agents killed", c.agentKillsTotal.Load())
	writeCounter(&b, "ughi_skill_executions_total", "Total skill executions", c.skillExecsTotal.Load())
	writeCounter(&b, "ughi_errors_total", "Total errors", c.errorsTotal.Load())

	// Counters by label
	b.WriteString("# HELP ughi_requests_by_method_total HTTP requests by method\n")
	b.WriteString("# TYPE ughi_requests_by_method_total counter\n")
	c.requestsByMethod.Range(func(key, value interface{}) bool {
		fmt.Fprintf(&b, "ughi_requests_by_method_total{method=\"%s\"} %d\n", key, value.(*atomic.Int64).Load())
		return true
	})

	b.WriteString("# HELP ughi_requests_by_status_total HTTP requests by status code\n")
	b.WriteString("# TYPE ughi_requests_by_status_total counter\n")
	c.requestsByStatus.Range(func(key, value interface{}) bool {
		fmt.Fprintf(&b, "ughi_requests_by_status_total{status=\"%s\"} %d\n", key, value.(*atomic.Int64).Load())
		return true
	})

	// Gauges
	writeGauge(&b, "ughi_agents_active", "Currently active agents", c.agentsActive.Load())
	writeGauge(&b, "ughi_agents_hibernated", "Currently hibernated agents", c.agentsHibernated.Load())
	writeGauge(&b, "ughi_memory_used_bytes", "Memory used in bytes", c.memoryUsedBytes.Load())
	writeGauge(&b, "ughi_goroutines", "Number of goroutines", c.goroutineCount.Load())
	writeGauge(&b, "ughi_websocket_clients", "Active WebSocket clients", c.wsClientsActive.Load())
	writeGauge(&b, "ughi_uptime_seconds", "Uptime in seconds", int64(time.Since(c.bootTime).Seconds()))

	// Histograms
	writeHistogram(&b, "ughi_request_latency_ms", "Request latency in milliseconds", c.requestLatencies)
	writeHistogram(&b, "ughi_agent_lifecycle_ms", "Agent lifecycle duration in milliseconds", c.agentLifecycleMs)

	return b.String()
}

func writeCounter(b *strings.Builder, name, help string, value int64) {
	fmt.Fprintf(b, "# HELP %s %s\n# TYPE %s counter\n%s %d\n", name, help, name, name, value)
}

func writeGauge(b *strings.Builder, name, help string, value int64) {
	fmt.Fprintf(b, "# HELP %s %s\n# TYPE %s gauge\n%s %d\n", name, help, name, name, value)
}

func writeHistogram(b *strings.Builder, name, help string, h *Histogram) {
	h.mu.Lock()
	defer h.mu.Unlock()

	fmt.Fprintf(b, "# HELP %s %s\n# TYPE %s histogram\n", name, help, name)
	cumulative := int64(0)
	for i, bucket := range h.buckets {
		cumulative += h.counts[i]
		fmt.Fprintf(b, "%s_bucket{le=\"%.0f\"} %d\n", name, bucket, cumulative)
	}
	cumulative += h.counts[len(h.buckets)]
	fmt.Fprintf(b, "%s_bucket{le=\"+Inf\"} %d\n", name, cumulative)
	fmt.Fprintf(b, "%s_sum %.2f\n", name, h.sum)
	fmt.Fprintf(b, "%s_count %d\n", name, h.count)
}
