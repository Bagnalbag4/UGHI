// Package dashboard serves the real-time HTML agent tree dashboard.
// Follows strict_rules.md | camelCase Go
// Memory cost: ~64 KB (embedded HTML/JS/CSS, static)
// The dashboard connects via WebSocket at /ws for real-time updates.
// Shows agent tree visualization similar to Antigravity right panel.
package dashboard

import "github.com/gofiber/fiber/v2"

// DashboardHTML is the complete real-time agent dashboard.
// Embedded as a Go string constant to avoid external file dependencies.
// Memory cost: ~32 KB (static, shared across all requests)
const DashboardHTML = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Agenticos – Live Dashboard</title>
<style>
  :root {
    --bg-primary: #0a0e17;
    --bg-secondary: #111827;
    --bg-card: #1a1f2e;
    --accent-cyan: #06d6a0;
    --accent-violet: #7c3aed;
    --accent-rose: #f43f5e;
    --accent-amber: #f59e0b;
    --text-primary: #f1f5f9;
    --text-secondary: #94a3b8;
    --border: #2d3748;
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: 'Inter', 'Segoe UI', system-ui, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    min-height: 100vh;
  }
  .header {
    background: linear-gradient(135deg, var(--bg-secondary), var(--bg-card));
    border-bottom: 1px solid var(--border);
    padding: 16px 24px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .header h1 {
    font-size: 20px;
    font-weight: 700;
    background: linear-gradient(135deg, var(--accent-cyan), var(--accent-violet));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .status-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent-rose);
    animation: pulse 2s infinite;
  }
  .status-dot.connected { background: var(--accent-cyan); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }

  .container { display: grid; grid-template-columns: 1fr 320px; gap: 16px; padding: 16px; height: calc(100vh - 64px); }
  .main-panel { display: flex; flex-direction: column; gap: 16px; overflow-y: auto; }
  .side-panel { display: flex; flex-direction: column; gap: 16px; overflow-y: auto; }

  .card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px;
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 12px;
  }

  .metrics-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
  }
  .metric {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 12px;
    text-align: center;
  }
  .metric-value {
    font-size: 24px;
    font-weight: 700;
    color: var(--accent-cyan);
  }
  .metric-label {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 4px;
  }
  .metric.warn .metric-value { color: var(--accent-amber); }
  .metric.danger .metric-value { color: var(--accent-rose); }

  .agent-tree { list-style: none; }
  .agent-node {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    margin-bottom: 8px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    transition: all 0.2s;
    animation: slideIn 0.3s ease-out;
  }
  .agent-node:hover { border-color: var(--accent-violet); transform: translateX(4px); }
  @keyframes slideIn { from { opacity: 0; transform: translateY(-8px); } to { opacity: 1; transform: translateY(0); } }

  .agent-info { flex: 1; }
  .agent-id {
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
    color: var(--accent-violet);
    font-weight: 600;
  }
  .agent-goal {
    font-size: 13px;
    color: var(--text-primary);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 300px;
  }
  .agent-meta {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    font-size: 10px;
    color: var(--text-secondary);
  }

  .state-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 12px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .state-spawned { background: #1e3a5f; color: #60a5fa; }
  .state-planning { background: #3b1f6e; color: #a78bfa; }
  .state-thinking { background: #164e3b; color: #34d399; }
  .state-tool_using { background: #78350f; color: #fbbf24; }
  .state-collaborating { background: #3b0764; color: #c084fc; }
  .state-reviewing { background: #1e3a5f; color: #38bdf8; }
  .state-completing { background: #064e3b; color: #6ee7b7; }
  .state-crashed { background: #7f1d1d; color: #fca5a5; }

  .event-log { max-height: 300px; overflow-y: auto; }
  .event-item {
    font-size: 11px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    animation: fadeIn 0.3s;
  }
  .event-item .time { color: var(--text-secondary); }
  .event-item .type { color: var(--accent-cyan); font-weight: 600; }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

  .spawn-form { display: flex; gap: 8px; }
  .spawn-form input {
    flex: 1;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 12px;
    color: var(--text-primary);
    font-size: 13px;
    outline: none;
  }
  .spawn-form input:focus { border-color: var(--accent-violet); }
  .spawn-form button {
    background: linear-gradient(135deg, var(--accent-violet), var(--accent-cyan));
    border: none;
    border-radius: 8px;
    padding: 8px 16px;
    color: white;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: transform 0.1s;
  }
  .spawn-form button:hover { transform: scale(1.05); }
  .spawn-form button:active { transform: scale(0.95); }

  .empty-state {
    text-align: center;
    padding: 40px;
    color: var(--text-secondary);
    font-size: 14px;
  }
  .empty-state .icon { font-size: 48px; margin-bottom: 12px; }

  @media (max-width: 768px) {
    .container { grid-template-columns: 1fr; }
    .metrics-grid { grid-template-columns: repeat(2, 1fr); }
  }
</style>
</head>
<body>

<div class="header">
  <h1>⚡ Agenticos Dashboard</h1>
  <div class="status-badge">
    <div class="status-dot" id="statusDot"></div>
    <span id="statusText">Connecting...</span>
  </div>
</div>

<div class="container">
  <div class="main-panel">
    <!-- Spawn Form -->
    <div class="card">
      <div class="card-title">🚀 Spawn Agent</div>
      <div class="spawn-form">
        <input type="text" id="goalInput" placeholder="Enter agent goal..." />
        <select id="prioritySelect" style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:8px;padding:8px;color:var(--text-primary);font-size:12px;">
          <option value="normal">Normal</option>
          <option value="high" selected>High</option>
          <option value="critical">Critical</option>
          <option value="low">Low</option>
          <option value="background">Background</option>
        </select>
        <button onclick="spawnAgent()">Spawn</button>
        <button onclick="runAgent()" style="background:linear-gradient(135deg,#f59e0b,#f43f5e);">Run</button>
      </div>
    </div>

    <!-- Metrics -->
    <div class="card">
      <div class="card-title">📊 Runtime Metrics</div>
      <div class="metrics-grid">
        <div class="metric"><div class="metric-value" id="mActive">0</div><div class="metric-label">Active</div></div>
        <div class="metric"><div class="metric-value" id="mSpawned">0</div><div class="metric-label">Spawned</div></div>
        <div class="metric"><div class="metric-value" id="mCompleted">0</div><div class="metric-label">Completed</div></div>
        <div class="metric warn"><div class="metric-value" id="mMemory">0</div><div class="metric-label">Memory MB</div></div>
      </div>
    </div>

    <!-- Agent Tree -->
    <div class="card" style="flex:1;">
      <div class="card-title">🌳 Agent Tree <span id="agentCount" style="color:var(--accent-cyan);">(0)</span></div>
      <ul class="agent-tree" id="agentTree">
        <li class="empty-state"><div class="icon">🤖</div>No agents yet. Spawn one above!</li>
      </ul>
    </div>
  </div>

  <div class="side-panel">
    <!-- Connection Info -->
    <div class="card">
      <div class="card-title">⚙️ System</div>
      <div style="font-size:12px;color:var(--text-secondary);">
        <div style="margin-bottom:4px;">Version: <span style="color:var(--accent-cyan);">0.1.0</span></div>
        <div style="margin-bottom:4px;">Kernel: <span id="kernelStatus" style="color:var(--accent-amber);">standalone</span></div>
        <div style="margin-bottom:4px;">Max Agents: <span style="color:var(--text-primary);">50</span></div>
        <div>RAM Limit: <span style="color:var(--text-primary);">320 MB</span></div>
      </div>
    </div>

    <!-- Event Log -->
    <div class="card" style="flex:1;">
      <div class="card-title">📡 Live Events</div>
      <div class="event-log" id="eventLog">
        <div class="event-item"><span class="time">--:--:--</span> <span class="type">BOOT</span> Dashboard ready</div>
      </div>
    </div>
  </div>
</div>

<script>
let ws;
let agents = {};
let events = [];

function connectWS() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  ws = new WebSocket(proto + '//' + location.host + '/ws');

  ws.onopen = () => {
    document.getElementById('statusDot').classList.add('connected');
    document.getElementById('statusText').textContent = 'Connected';
    addEvent('WS', 'WebSocket connected');
  };

  ws.onclose = () => {
    document.getElementById('statusDot').classList.remove('connected');
    document.getElementById('statusText').textContent = 'Disconnected';
    addEvent('WS', 'WebSocket disconnected – reconnecting...');
    setTimeout(connectWS, 2000);
  };

  ws.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      handleMessage(data);
    } catch(err) { console.error('WS parse error:', err); }
  };
}

function handleMessage(data) {
  if (data.agents) {
    // Full state update
    agents = data.agents || {};
    if (data.metrics) updateMetrics(data.metrics);
    renderAgents();
  } else if (data.type) {
    // Tree event
    addEvent(data.type.toUpperCase(), data.nodeId + ' → ' + (data.newState || ''));
    refreshAgents();
  } else if (data.event_type) {
    // Kernel event
    addEvent(data.event_type.toUpperCase(), data.agent_id + (data.goal ? ': ' + data.goal : ''));
    refreshAgents();
  }
}

function refreshAgents() {
  fetch('/api/agents').then(r => r.json()).then(data => {
    if (data.agents) {
      agents = {};
      data.agents.forEach(a => { agents[a.id] = a; });
      renderAgents();
    }
    document.getElementById('agentCount').textContent = '(' + (data.count || 0) + ')';
  }).catch(() => {});

  fetch('/api/metrics').then(r => r.json()).then(updateMetrics).catch(() => {});
}

function renderAgents() {
  const tree = document.getElementById('agentTree');
  const ids = Object.keys(agents);

  if (ids.length === 0) {
    tree.innerHTML = '<li class="empty-state"><div class="icon">🤖</div>No agents yet. Spawn one above!</li>';
    document.getElementById('agentCount').textContent = '(0)';
    return;
  }

  document.getElementById('agentCount').textContent = '(' + ids.length + ')';
  tree.innerHTML = ids.map(id => {
    const a = agents[id];
    return '<li class="agent-node">' +
      '<div class="agent-info">' +
        '<div class="agent-id">' + a.id + '</div>' +
        '<div class="agent-goal">' + escHtml(a.goal) + '</div>' +
        '<div class="agent-meta">' +
          '<span>⏱ ' + (a.uptime_ms||0) + 'ms</span>' +
          '<span>↔ ' + (a.transition_count||0) + ' transitions</span>' +
          '<span>🔒 ' + (a.capabilities_count||0) + ' caps</span>' +
        '</div>' +
      '</div>' +
      '<div style="display:flex;gap:6px;align-items:center;">' +
        '<span class="state-badge state-' + a.state + '">' + a.state + '</span>' +
        '<button onclick="killAgent(\'' + a.id + '\')" style="background:#7f1d1d;border:none;color:#fca5a5;border-radius:6px;padding:4px 8px;font-size:10px;cursor:pointer;">✕</button>' +
      '</div>' +
    '</li>';
  }).join('');
}

function updateMetrics(m) {
  document.getElementById('mActive').textContent = m.agents_active || 0;
  document.getElementById('mSpawned').textContent = m.agents_total_spawned || 0;
  document.getElementById('mCompleted').textContent = m.agents_total_completed || 0;
  document.getElementById('mMemory').textContent = (m.memory_total_mb || 0).toFixed(1);
}

function addEvent(type, msg) {
  const now = new Date().toLocaleTimeString('en-US', {hour12:false});
  const log = document.getElementById('eventLog');
  const item = document.createElement('div');
  item.className = 'event-item';
  item.innerHTML = '<span class="time">' + now + '</span> <span class="type">' + type + '</span> ' + escHtml(msg);
  log.insertBefore(item, log.firstChild);
  if (log.children.length > 50) log.removeChild(log.lastChild);
}

function spawnAgent() {
  const goal = document.getElementById('goalInput').value.trim();
  const priority = document.getElementById('prioritySelect').value;
  if (!goal) return;

  fetch('/api/spawn', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({goal, priority})
  }).then(r => r.json()).then(data => {
    addEvent('SPAWN', data.agent_id + ': ' + goal);
    document.getElementById('goalInput').value = '';
    refreshAgents();
  }).catch(err => addEvent('ERROR', err.message));
}

function runAgent() {
  const goal = document.getElementById('goalInput').value.trim();
  const priority = document.getElementById('prioritySelect').value;
  if (!goal) return;

  addEvent('RUN', 'Starting one-shot: ' + goal);
  fetch('/api/run', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({goal, priority})
  }).then(r => r.json()).then(data => {
    if (data.result) {
      addEvent('DONE', data.result.id + ': ' + data.result.goal + ' (' + data.result.transition_count + ' transitions)');
    }
    document.getElementById('goalInput').value = '';
    refreshAgents();
  }).catch(err => addEvent('ERROR', err.message));
}

function killAgent(id) {
  fetch('/api/kill/' + id, {method: 'POST'}).then(r => r.json()).then(data => {
    addEvent('KILL', id);
    refreshAgents();
  }).catch(err => addEvent('ERROR', err.message));
}

function escHtml(s) { return s ? s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])) : ''; }

// Boot
connectWS();
setInterval(refreshAgents, 5000);
</script>
</body>
</html>`

// RegisterDashboard registers the dashboard route on a Fiber app.
// Memory cost: 0 (static HTML response)
func RegisterDashboard(app *fiber.App) {
	app.Get("/dashboard", func(c *fiber.Ctx) error {
		c.Set("Content-Type", "text/html; charset=utf-8")
		return c.SendString(DashboardHTML)
	})
}
