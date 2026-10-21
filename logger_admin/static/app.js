/* ── State ──────────────────────────────────────────────────────────────── */
const state = {
  token: sessionStorage.getItem('admin_token') || '',
  config: null,        // ConfigResponse from GET /api/config
  dirty: false,
};

/* ── DOM refs ───────────────────────────────────────────────────────────── */
const $ = id => document.getElementById(id);

const authOverlay   = $('auth-overlay');
const authForm      = $('auth-form');
const authError     = $('auth-error');
const tokenInput    = $('token-input');
const app           = $('app');

const reloadBtn     = $('reload-btn');
const logoutBtn     = $('logout-btn');
const statusBadge   = $('status-badge');
const consulIndex   = $('consul-index');

const saveBar       = $('save-bar');
const saveBtn       = $('save-btn');
const discardBtn    = $('discard-btn');
const addChannelBtn = $('add-channel-btn');
const channelsList  = $('channels-list');
const noChannels    = $('no-channels');
const toast         = $('toast');

const gIssuer    = $('g-issuer');
const gTtl       = $('g-ttl');
const gMaxBody   = $('g-max-body');

/* ── Toast ──────────────────────────────────────────────────────────────── */
let toastTimer;
function showToast(msg, type = 'success') {
  toast.textContent = msg;
  toast.className = `toast ${type}`;
  toast.classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.add('hidden'), 4000);
}

/* ── Auth ───────────────────────────────────────────────────────────────── */
function showAuth() {
  authOverlay.classList.remove('hidden');
  app.classList.add('hidden');
}

function showApp() {
  authOverlay.classList.add('hidden');
  app.classList.remove('hidden');
}

authForm.addEventListener('submit', async e => {
  e.preventDefault();
  const token = tokenInput.value.trim();
  if (!token) return;

  // Probe the API to validate the token before storing it.
  const res = await fetch('/api/config', {
    headers: { Authorization: `Bearer ${token}` }
  });

  if (res.status === 401) {
    authError.textContent = 'Invalid token. Please try again.';
    authError.classList.remove('hidden');
    return;
  }
  if (!res.ok) {
    authError.textContent = `Server error (${res.status}). Try again later.`;
    authError.classList.remove('hidden');
    return;
  }

  authError.classList.add('hidden');
  state.token = token;
  sessionStorage.setItem('admin_token', token);
  const data = await res.json();
  applyConfig(data);
  showApp();
});

logoutBtn.addEventListener('click', () => {
  sessionStorage.removeItem('admin_token');
  state.token = '';
  state.config = null;
  tokenInput.value = '';
  showAuth();
});

/* ── API helpers ─────────────────────────────────────────────────────────── */
async function apiGet(path) {
  const res = await fetch(path, {
    headers: { Authorization: `Bearer ${state.token}` }
  });
  if (res.status === 401) { showAuth(); throw new Error('Unauthorized'); }
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}

async function apiPost(path, body) {
  const res = await fetch(path, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${state.token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body),
  });
  if (res.status === 401) { showAuth(); throw new Error('Unauthorized'); }
  return res;
}

async function apiDelete(path) {
  const res = await fetch(path, {
    method: 'DELETE',
    headers: { Authorization: `Bearer ${state.token}` }
  });
  if (res.status === 401) { showAuth(); throw new Error('Unauthorized'); }
  if (!res.ok) throw new Error(`DELETE ${path} failed: HTTP ${res.status}`);
}

/* ── Load config ─────────────────────────────────────────────────────────── */
async function loadConfig() {
  try {
    setStatus('loading');
    const data = await apiGet('/api/config');
    applyConfig(data);
    setStatus('ok');
  } catch (err) {
    setStatus('error');
    showToast(`Failed to load config: ${err.message}`, 'error');
  }
}

function applyConfig(data) {
  state.config = data;
  state.dirty = false;
  saveBar.classList.add('hidden');

  // Global fields
  const g = data.global;
  gIssuer.value    = g.jwt.issuer     || '';
  gTtl.value       = g.jwt.ttl_seconds || 900;
  gMaxBody.value   = g.max_body_bytes  || 2097152;

  consulIndex.textContent = `idx: ${data.global_modify_index}`;

  renderChannels(data.channels);
}

/* ── Channels rendering ──────────────────────────────────────────────────── */
function renderChannels(channels) {
  channelsList.innerHTML = '';

  if (!channels || channels.length === 0) {
    noChannels.classList.remove('hidden');
    return;
  }
  noChannels.classList.add('hidden');

  channels.forEach(item => {
    channelsList.appendChild(buildChannelRow(item.channel || item, item.modify_index));
  });
}

function buildChannelRow(ch, modifyIndex = 0, isNew = false) {
  const row = document.createElement('div');
  row.className = `channel-item${isNew ? ' is-new' : ''}`;
  row.dataset.id = ch.id || '';
  row.dataset.modifyIndex = modifyIndex;

  const androidMode  = ch.android_attestation_mode || 'off';
  const iosMode      = ch.ios_attestation_mode     || 'off';
  const androidCerts = Array.isArray(ch.android_signing_cert_sha256)
    ? ch.android_signing_cert_sha256.join(', ')
    : '';
  const appleEnv     = ch.apple_app_attest_env || '';

  row.innerHTML = `
    <div class="channel-fields">
      <div class="field-group">
        <label>Channel ID</label>
        <input class="ch-id mono" type="text" value="${esc(ch.id)}" placeholder="example-app-prod" />
      </div>
      <div class="field-group">
        <label>App ID</label>
        <input class="ch-appid mono" type="text" value="${esc(ch.app_id)}" placeholder="dev.femrek.exampleapp" />
      </div>
      <div class="field-group">
        <label>Android Signing Cert SHA-256 <span class="hint">Key Attestation (optional, comma-separated hex)</span></label>
        <input class="ch-android-certs mono" type="text" value="${esc(androidCerts)}" placeholder="optional — skip cert check if empty" />
      </div>
      <div class="field-group">
        <label>Apple Team ID <span class="hint">App Attest</span></label>
        <input class="ch-apple-team mono" type="text" value="${esc(ch.apple_team_id)}" placeholder="optional" />
      </div>
      <div class="field-group">
        <label>Apple App Attest Env <span class="hint">App Attest</span></label>
        <select class="ch-apple-env">
          <option value=""${appleEnv === '' ? ' selected' : ''}>Any (unset)</option>
          <option value="development"${appleEnv === 'development' ? ' selected' : ''}>Development</option>
          <option value="production"${appleEnv === 'production' ? ' selected' : ''}>Production</option>
        </select>
      </div>
      <div class="field-group">
        <label>Android Mode</label>
        <select class="ch-android-mode">
          <option value="off"${androidMode === 'off' ? ' selected' : ''}>Off</option>
          <option value="monitor"${androidMode === 'monitor' ? ' selected' : ''}>Monitor</option>
          <option value="enforce"${androidMode === 'enforce' ? ' selected' : ''}>Enforce</option>
        </select>
      </div>
      <div class="field-group">
        <label>iOS Mode</label>
        <select class="ch-ios-mode">
          <option value="off"${iosMode === 'off' ? ' selected' : ''}>Off</option>
          <option value="monitor"${iosMode === 'monitor' ? ' selected' : ''}>Monitor</option>
          <option value="enforce"${iosMode === 'enforce' ? ' selected' : ''}>Enforce</option>
        </select>
      </div>
    </div>
    <div class="channel-actions">
      <button class="btn btn-danger btn-delete" title="Delete this channel">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/>
          <path d="M10 11v6M14 11v6"/><path d="M9 6V4h6v2"/>
        </svg>
      </button>
    </div>
  `;

  row.querySelector('.btn-delete').addEventListener('click', () => deleteChannelRow(row, ch.id));
  row.querySelectorAll('input').forEach(input => input.addEventListener('input', markDirty));
  row.querySelectorAll('select').forEach(sel => sel.addEventListener('change', markDirty));

  return row;
}

async function deleteChannelRow(row, channelId) {
  if (!channelId) {
    // Unsaved new row — just remove from DOM.
    row.remove();
    updateEmptyState();
    markDirty();
    return;
  }

  if (!confirm(`Delete channel "${channelId}"? This takes effect immediately in Consul.`)) return;

  try {
    await apiDelete(`/api/config/channels/${channelId}`);
    row.remove();
    updateEmptyState();
    showToast(`Channel "${channelId}" deleted`, 'success');
    // Reload to refresh indexes.
    await loadConfig();
  } catch (err) {
    showToast(`Failed to delete: ${err.message}`, 'error');
  }
}

addChannelBtn.addEventListener('click', () => {
  noChannels.classList.add('hidden');
  const row = buildChannelRow(
    { id: '', app_id: '', android_attestation_mode: 'off', ios_attestation_mode: 'off',
      android_signing_cert_sha256: [], apple_team_id: '', apple_app_attest_env: '' },
    0, true
  );
  channelsList.appendChild(row);
  row.querySelector('.ch-id').focus();
  markDirty();
});

function updateEmptyState() {
  const hasRows = channelsList.children.length > 0;
  noChannels.classList.toggle('hidden', hasRows);
}

/* ── Dirty tracking ──────────────────────────────────────────────────────── */
[gIssuer, gTtl, gMaxBody].forEach(el =>
  el.addEventListener('input', markDirty)
);

function markDirty() {
  if (!state.dirty) {
    state.dirty = true;
    saveBar.classList.remove('hidden');
  }
}

discardBtn.addEventListener('click', () => {
  if (state.config) applyConfig(state.config);
});

/* ── Save ────────────────────────────────────────────────────────────────── */
saveBtn.addEventListener('click', async () => {
  saveBtn.disabled = true;
  try {
    const payload = buildSavePayload();
    const res = await apiPost('/api/config', payload);

    if (res.status === 409) {
      showToast('Concurrent edit conflict — reloading config. Please re-apply your changes.', 'warning');
      await loadConfig();
      return;
    }
    if (!res.ok) {
      const body = await res.text();
      showToast(`Save failed (${res.status}): ${body}`, 'error');
      return;
    }

    showToast('Config saved to Consul KV ✓', 'success');
    await loadConfig();
  } catch (err) {
    showToast(`Save error: ${err.message}`, 'error');
  } finally {
    saveBtn.disabled = false;
  }
});

function buildSavePayload() {
  const channelRows = [...channelsList.querySelectorAll('.channel-item')];
  const channels = channelRows.map(row => {
    const rawCerts   = row.querySelector('.ch-android-certs').value;
    const androidCerts = rawCerts
      .split(',')
      .map(s => s.trim())
      .filter(Boolean);
    const appleTeam  = row.querySelector('.ch-apple-team').value.trim();
    const appleEnv   = row.querySelector('.ch-apple-env').value.trim();
    return {
      id:                       row.querySelector('.ch-id').value.trim(),
      app_id:                   row.querySelector('.ch-appid').value.trim(),
      android_attestation_mode: row.querySelector('.ch-android-mode').value,
      ios_attestation_mode:     row.querySelector('.ch-ios-mode').value,
      ...(androidCerts.length > 0 ? { android_signing_cert_sha256: androidCerts } : {}),
      ...(appleTeam  ? { apple_team_id:        appleTeam } : {}),
      ...(appleEnv   ? { apple_app_attest_env: appleEnv  } : {}),
    };
  });

  // Build modifyIndex map from current config.
  const channelModifyIndexes = {};
  if (state.config) {
    for (const item of state.config.channels) {
      channelModifyIndexes[item.channel?.id ?? item.id] = item.modify_index ?? 0;
    }
  }

  return {
    global: {
      jwt: {
        issuer:         gIssuer.value.trim()    || 'logger-gateway',
        ttl_seconds:    parseInt(gTtl.value)    || 900,
      },
      max_body_bytes: parseInt(gMaxBody.value) || 2097152,
    },
    global_modify_index: state.config?.global_modify_index ?? 0,
    channels,
    channel_modify_indexes: channelModifyIndexes,
  };
}

/* ── Reload ──────────────────────────────────────────────────────────────── */
reloadBtn.addEventListener('click', loadConfig);

/* ── Status badge ────────────────────────────────────────────────────────── */
function setStatus(s) {
  statusBadge.textContent = s === 'ok' ? 'Connected' : s === 'error' ? 'Error' : '…';
  statusBadge.className   = `badge${s === 'error' ? ' error' : ''}`;
}

/* ── Helpers ─────────────────────────────────────────────────────────────── */
function esc(str) {
  return (str || '').replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

/* ── Bootstrap ───────────────────────────────────────────────────────────── */
if (state.token) {
  showApp();
  loadConfig();
} else {
  showAuth();
}
