// AdaTP Silo Panel — talks exclusively to the /admin/v1 API.
'use strict';

const $ = (id) => document.getElementById(id);
let TOKEN = sessionStorage.getItem('adatp_admin_token') || '';
let currentView = 'overview';
let pollTimer = null;
let logSource = null;

const api = async (path, options = {}) => {
    const res = await fetch(`/admin/v1${path}`, {
        ...options,
        headers: {
            'x-admin-token': TOKEN,
            ...(options.body ? { 'content-type': 'application/json' } : {}),
            ...(options.headers || {}),
        },
    });
    if (res.status === 401) { lock(); throw new Error('unauthorized'); }
    return res.json();
};

// ---------------------------------------------------------------- login
function lock() {
    sessionStorage.removeItem('adatp_admin_token');
    TOKEN = '';
    if (logSource) { logSource.close(); logSource = null; }
    clearInterval(pollTimer);
    $('app').style.display = 'none';
    $('login').style.display = 'flex';
}

async function unlock(token) {
    TOKEN = token;
    try {
        await api('/overview');
    } catch {
        $('loginError').textContent = t('denied');
        return;
    }
    sessionStorage.setItem('adatp_admin_token', token);
    $('login').style.display = 'none';
    $('app').style.display = 'block';
    startPolling();
}

$('loginBtn').onclick = () => unlock($('tokenInput').value.trim());
$('tokenInput').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('loginBtn').click(); });
$('logoutBtn').onclick = lock;

// ---------------------------------------------------------------- nav
document.querySelectorAll('#nav button').forEach(btn => {
    btn.onclick = () => {
        document.querySelectorAll('#nav button').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        document.querySelectorAll('.view').forEach(v => v.style.display = 'none');
        currentView = btn.dataset.view;
        $(`view-${currentView}`).style.display = 'grid';
        if (currentView === 'logs') attachLogStream(); else refresh();
    };
});

setInterval(() => { $('clock').textContent = new Date().toISOString().replace('T', ' ').slice(0, 19) + 'Z'; }, 1000);

// ---------------------------------------------------------------- polling
function startPolling() {
    refresh();
    clearInterval(pollTimer);
    pollTimer = setInterval(refresh, 2000);
}

async function refresh() {
    try {
        if (currentView === 'overview') await renderOverview();
        if (currentView === 'connections') await renderConnections();
        if (currentView === 'rooms') await renderRooms();
        if (currentView === 'webhooks') await renderWebhooks();
        if (currentView === 'plugins') await renderPlugins();
        if (currentView === 'settings') await renderSettings();
    } catch (e) { /* 401 already handled */ }
}

// ---------------------------------------------------------------- views
const fmtBytes = (n) => {
    if (n >= 1 << 20) return (n / (1 << 20)).toFixed(1) + ' MB/s';
    if (n >= 1 << 10) return (n / (1 << 10)).toFixed(1) + ' kB/s';
    return n + ' B/s';
};
const fmtUptime = (s) => {
    const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60);
    return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m ${s % 60}s`;
};
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

async function renderOverview() {
    const o = await api('/overview');
    $('kpiConns').textContent = o.connections.active;
    $('kpiRooms').textContent = o.rooms;
    $('kpiUptime').textContent = fmtUptime(o.uptime_seconds);
    $('kpiDropped').textContent = o.connections.dropped_messages;
    $('kpiPlugins').textContent = `${o.plugins.running}/${o.plugins.total}`;
    $('kpiWebhooks').textContent = `${o.webhooks.active}/${o.webhooks.total}`;
    $('statusLed').className = 'led ' + (o.draining ? 'led-amber' : 'led-green');

    const load = await api('/load');
    const cur = load.current;
    $('kpiRx').textContent = cur ? fmtBytes(cur.rx_bytes_per_s) : '–';
    $('kpiTx').textContent = cur ? fmtBytes(cur.tx_bytes_per_s) : '–';
    drawChart(load.series || []);

    const lb = await api('/lb-hints');
    $('lbHints').textContent =
        `healthy=${lb.healthy}  draining=${lb.draining}  connections=${lb.connections}/${lb.max_connections}  capacity=${lb.capacity_used_pct}%`;
}

function drawChart(series) {
    const svg = $('loadChart');
    if (!series.length) { svg.innerHTML = ''; return; }
    const W = 600, H = 140, PAD = 4;
    const max = Math.max(1, ...series.map(s => Math.max(s.rx_bytes_per_s, s.tx_bytes_per_s)));
    const x = (i) => PAD + i * (W - 2 * PAD) / Math.max(1, series.length - 1);
    const y = (v) => H - PAD - v * (H - 2 * PAD) / max;
    const path = (key) => series.map((s, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)},${y(s[key]).toFixed(1)}`).join(' ');
    svg.innerHTML =
        `<path d="${path('rx_bytes_per_s')}" fill="none" stroke="#4fc3f7" stroke-width="1.5"/>` +
        `<path d="${path('tx_bytes_per_s')}" fill="none" stroke="#ffb547" stroke-width="1.5"/>`;
}

async function renderConnections() {
    const { connections } = await api('/connections');
    $('connCount').textContent = `(${connections.length})`;
    $('connTable').innerHTML = connections.map(c => `
        <tr><td>${c.id}</td><td>${esc(c.username)}</td><td>${esc(c.role)}</td>
        <td>${esc(c.room)}</td><td>${esc(c.remote)}</td>
        <td>${new Date(c.connected_at_ms).toISOString().slice(11, 19)}</td>
        <td><button class="mini danger" onclick="kickConn(${c.id})">${t('kick')}</button></td></tr>`).join('');
}
window.kickConn = async (id) => { await api(`/connections/${id}`, { method: 'DELETE' }); refresh(); };

async function renderRooms() {
    const { rooms } = await api('/rooms');
    $('roomTable').innerHTML = rooms
        .sort((a, b) => b.members - a.members)
        .map(r => `<tr><td>${esc(r.name)}</td><td>${r.members}</td></tr>`).join('');
}

function attachLogStream() {
    if (logSource) return;
    api('/logs').then(({ lines }) => {
        $('logStream').innerHTML = lines.map(fmtLog).join('');
        scrollLogs();
    });
    logSource = new EventSource(`/admin/v1/logs/stream?token=${encodeURIComponent(TOKEN)}`);
    logSource.onmessage = (e) => {
        const el = $('logStream');
        el.insertAdjacentHTML('beforeend', fmtLog(JSON.parse(e.data)));
        while (el.children.length > 600) el.removeChild(el.firstChild);
        scrollLogs();
    };
    logSource.onerror = () => { logSource.close(); logSource = null; };
}
const fmtLog = (l) =>
    `<div class="log-${l.level}"><span class="log-time">${new Date(l.at_ms).toISOString().slice(11, 23)}</span> ` +
    `${l.level.padEnd(5)} <span class="dim">${esc(l.target)}</span> ${esc(l.message)}</div>`;
const scrollLogs = () => { const el = $('logStream'); el.scrollTop = el.scrollHeight; };

async function renderWebhooks() {
    const { webhooks } = await api('/webhooks');
    $('whTable').innerHTML = webhooks.map(w => `
        <tr><td><span class="led ${w.breaker_open ? 'led-red' : w.is_active ? 'led-green' : 'led-amber'}"></span></td>
        <td title="${esc(w.description || '')}">${esc(w.url)}</td>
        <td>${w.events.map(e => `<span class="tag">${esc(e)}</span>`).join('')}</td>
        <td>${w.delivered}</td><td>${w.failed}</td><td>${w.skipped_breaker}</td>
        <td>
          <button class="mini" onclick="whTest('${w.id}')">${t('test')}</button>
          <button class="mini" onclick="whToggle('${w.id}', ${!w.is_active})">${w.is_active ? t('pause') : t('resume')}</button>
          <button class="mini danger" onclick="whDelete('${w.id}')">${t('del')}</button>
        </td></tr>`).join('');

    const { audit } = await api('/webhooks/audit');
    $('whAudit').innerHTML = audit.slice(-40).reverse().map(a => `
        <tr><td>${new Date(a.at_ms).toISOString().slice(11, 19)}</td>
        <td>${a.endpoint_id.slice(0, 8)}</td><td>${esc(a.event)}</td>
        <td class="${a.outcome === 'delivered' ? '' : 'log-WARN'}">${a.outcome}</td>
        <td>${a.status ?? '–'}</td><td>${a.attempt}</td></tr>`).join('');
}
window.whTest = async (id) => { await api(`/webhooks/${id}/test`, { method: 'POST' }); };
window.whToggle = async (id, active) => {
    await api(`/webhooks/${id}`, { method: 'PATCH', body: JSON.stringify({ active }) });
    refresh();
};
window.whDelete = async (id) => {
    if (confirm('Delete this webhook endpoint?')) {
        await api(`/webhooks/${id}`, { method: 'DELETE' });
        refresh();
    }
};
$('whCreate').onclick = async () => {
    const events = $('whEvents').value.split(',').map(s => s.trim()).filter(Boolean);
    const out = await api('/webhooks', {
        method: 'POST',
        body: JSON.stringify({
            url: $('whUrl').value.trim(),
            events,
            description: $('whDesc').value.trim() || null,
        }),
    });
    $('whCreateOut').textContent = out.ok
        ? `CREATED ${out.id} — signing secret (shown once): ${out.secret}`
        : `ERROR: ${out.error} ${out.detail || ''}`;
    refresh();
};

async function renderPlugins() {
    const { plugins } = await api('/plugins');
    $('pluginCards').innerHTML = plugins.map(p => `
        <div class="panel plugin-card">
          <span class="state state-${p.state}">${p.state.toUpperCase()}</span>
          <h3>${esc(p.name)} <span class="dim">v${esc(p.version)}</span></h3>
          <div class="meta">${esc(p.description)}</div>
          <div>${p.tools.map(t => `<span class="tag">${esc(t)}</span>`).join('')}
               ${p.hooks.map(h => `<span class="tag" style="color:var(--amber)">hook:${esc(h)}</span>`).join('')}</div>
          <div class="meta" style="margin-top:8px">
            calls=${p.calls} errors=${p.errors} restarts=${p.restarts} avg=${p.avg_latency_ms}ms
            ${p.last_error ? `<div class="log-ERROR">${esc(p.last_error)}</div>` : ''}
          </div>
          <div class="form-row" style="margin-top:8px">
            ${p.state === 'running'
                ? `<button class="mini" onclick="pluginAction('${p.name}','disable')">${t('disable')}</button>`
                : `<button class="mini" onclick="pluginAction('${p.name}','enable')">${t('enable')}</button>`}
            <button class="mini" onclick="pluginAction('${p.name}','reload')">${t('reload')}</button>
          </div>
        </div>`).join('');
}
window.pluginAction = async (name, action) => {
    await api(`/plugins/${name}/${action}`, { method: 'POST' });
    refresh();
};

async function renderSettings() {
    const cfg = await api('/config');
    $('configDump').textContent = JSON.stringify(cfg, null, 2);
}
$('drainOn').onclick = async () => { await api('/drain', { method: 'POST', body: JSON.stringify({ enabled: true }) }); refresh(); };
$('drainOff').onclick = async () => { await api('/drain', { method: 'POST', body: JSON.stringify({ enabled: false }) }); refresh(); };
$('usersReload').onclick = async () => { const r = await api('/users/reload', { method: 'POST' }); alert(r.ok ? `Reloaded ${r.users} user(s)` : r.error); };

// language switcher
siloInitLang();

// auto-login if a token is stored
if (TOKEN) unlock(TOKEN);
