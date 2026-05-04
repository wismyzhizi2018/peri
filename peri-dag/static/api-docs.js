// ── API Docs Modal ──────────────────────────────────────────────────

let _apiEndpoints = null;

function esc(s) { return (s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

function curlBlock(code) {
  return `<div class="api-curl"><button class="curl-copy" data-curl="${esc(code)}" onclick="copyCurl(this)">Copy</button>${esc(code)}</div>`;
}

function copyCurl(btn) {
  const text = btn.dataset.curl;
  if (!text) return;
  navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard', 'success'));
}

function closeApiModal() {
  document.getElementById('api-modal').style.display = 'none';
}

async function fetchApiEndpoints() {
  if (_apiEndpoints) return _apiEndpoints;
  const res = await fetch('/api/v1/docs');
  const data = await res.json();
  _apiEndpoints = data.endpoints || [];
  return _apiEndpoints;
}

function renderEndpoint(ep, host) {
  const methodClass = ep.method === 'GET' ? 'method-get' : 'method-post';
  const curl = ep.curl.replace(/\$HOST/g, host);
  let paramsHtml = '';
  if (ep.params && ep.params.length > 0) {
    paramsHtml = `<table class="api-param-table">
      <tr><th>Field</th><th>Type</th><th>Description</th></tr>
      ${ep.params.map(p => `<tr><td><code>${esc(p.name)}</code></td><td>${esc(p.type)}</td><td>${esc(p.description)}</td></tr>`).join('')}
    </table>`;
  }
  return `<div class="api-section">
    <h3><span class="method ${methodClass}">${esc(ep.method)}</span> ${esc(ep.path)}</h3>
    <p>${esc(ep.description)}</p>
    ${paramsHtml}
    ${curlBlock(curl)}
    <div class="api-response">// Response
${esc(ep.response)}</div>
  </div>`;
}

async function renderApiDocs(templateName) {
  const H = location.host;
  const body = document.getElementById('api-docs-body');
  body.innerHTML = '<div style="text-align:center;padding:24px;color:#656d76"><span class="spinner"></span> Loading...</div>';

  let endpoints;
  try {
    endpoints = await fetchApiEndpoints();
  } catch (e) {
    body.innerHTML = '<div style="color:#cf222e;padding:16px">Failed to load API docs</div>';
    return;
  }

  const host = location.host;
  let html;

  if (templateName) {
    // Template-specific: show only template-category endpoints, fill in the template name
    const filtered = endpoints.filter(ep => ep.category === 'templates');
    html = filtered.map(ep => {
      const concrete = { ...ep };
      concrete.path = ep.path.replace('{name}', templateName);
      concrete.curl = ep.curl.replace(/\$HOST/g, host).replace(/\{name\}/g, templateName);
      return renderEndpoint(concrete, host);
    }).join('');
  } else {
    html = endpoints.map(ep => renderEndpoint(ep, host)).join('');
  }

  body.innerHTML = html;
}

function showAllApiDocs() {
  document.getElementById('api-modal').style.display = '';
  document.getElementById('api-modal-title').textContent = 'API Reference';
  renderApiDocs(null);
}

function showTemplateApi(templateName) {
  document.getElementById('api-modal').style.display = '';
  document.getElementById('api-modal-title').textContent = 'API \u2014 ' + templateName;
  renderApiDocs(templateName);
}
