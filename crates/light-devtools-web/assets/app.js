// Light DevTools · Web UI (ТЗ 0.18.0) — plain JS, без фреймворков.
// Связь с бэкендом: fetch('/api/*') + EventSource('/api/events').

(() => {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const treeEl = $("tree");
  const stateEl = $("state");
  const logEl = $("log");
  const badge = $("status-badge");
  const btnRefresh = $("btn-refresh");
  const btnClick = $("btn-click");
  const btnShot = $("btn-shot");
  const btnClearLog = $("btn-clear-log");

  let selectedKey = null; // ключ выбранного узла VDOM ("0.0.1" и т.п.)
  let lastTreeAt = null;
  let lastStateAt = null;

  // ---------- логи ----------------------------------------------------------
  function log(kind, msg) {
    const t = new Date().toTimeString().slice(0, 8);
    const line = document.createElement("div");
    line.className = `line ${kind}`;
    line.innerHTML = `<span class="t">${t}</span>${escapeHtml(msg)}`;
    logEl.appendChild(line);
    logEl.scrollTop = logEl.scrollHeight;
    while (logEl.children.length > 500) logEl.removeChild(logEl.firstChild);
  }
  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  // ---------- статус бейджа -------------------------------------------------
  function setBadge(kind, text) {
    badge.className = `badge ${kind}`;
    badge.textContent = text;
  }

  // ---------- дерево --------------------------------------------------------
  function renderTree(data) {
    if (!data || typeof data !== "object") {
      treeEl.innerHTML = `<div class="placeholder">Данные дерева недоступны: ${escapeHtml(String(data))}</div>`;
      return;
    }
    treeEl.innerHTML = "";
    const ul = buildNode(data, "0", []);
    treeEl.appendChild(ul);
    $("tree-meta").textContent = `${countNodes(data)} узлов`;
  }

  function countNodes(node) {
    if (!node || typeof node !== "object") return 0;
    const kids = Array.isArray(node.children) ? node.children : [];
    return 1 + kids.reduce((acc, c) => acc + countNodes(c), 0);
  }

  function buildNode(node, keyPath, parentKeys) {
    const li = document.createElement("li");
    const row = document.createElement("div");
    row.className = "node-row";

    const tag = node.tag || node.type || "node";
    const hasKids = Array.isArray(node.children) && node.children.length > 0;

    if (hasKids) {
      const toggle = document.createElement("span");
      toggle.className = "toggle";
      toggle.textContent = "▾";
      row.appendChild(toggle);
    } else {
      const spacer = document.createElement("span");
      spacer.className = "toggle";
      spacer.innerHTML = "&nbsp;";
      row.appendChild(spacer);
    }

    const tagEl = document.createElement("span");
    tagEl.className = "tag";
    tagEl.textContent = tag;
    row.appendChild(tagEl);

    if (keyPath) {
      const keyEl = document.createElement("span");
      keyEl.className = "key";
      keyEl.textContent = keyPath;
      row.appendChild(keyEl);
    }

    const attrs = node.attrs || node.props || {};
    if (Object.keys(attrs).length) {
      const a = document.createElement("span");
      a.className = "attrs";
      a.textContent = " " + Object.entries(attrs)
        .map(([k, v]) => `${k}=${typeof v === "string" ? `"${v}"` : JSON.stringify(v)}`)
        .join(" ");
      row.appendChild(a);
    }

    // Клик по строке — выбрать ключ (для «Click selected»).
    row.addEventListener("click", () => {
      selectKey(keyPath);
    });

    li.appendChild(row);

    if (hasKids) {
      const ul = document.createElement("ul");
      node.children.forEach((c, i) => {
        ul.appendChild(buildNode(c, `${keyPath}.${i}`, [...parentKeys, keyPath]));
      });
      // Сворачивание: клик по toggle.
      const toggle = row.querySelector(".toggle");
      if (toggle && keyPath) {
        toggle.style.cursor = "pointer";
        toggle.addEventListener("click", (e) => {
          e.stopPropagation();
          const open = ul.style.display !== "none";
          ul.style.display = open ? "none" : "";
          toggle.textContent = open ? "▸" : "▾";
        });
      }
      li.appendChild(ul);
    }
    return li;
  }

  function selectKey(key) {
    if (!key) {
      selectedKey = null;
      btnClick.disabled = true;
      document.querySelectorAll(".node-row.selected").forEach((n) => n.classList.remove("selected"));
      return;
    }
    selectedKey = key;
    btnClick.disabled = false;
    document.querySelectorAll(".node-row.selected").forEach((n) => n.classList.remove("selected"));
    // Найти строку, содержащий точно этот ключ.
    document.querySelectorAll(".node-row").forEach((row) => {
      const k = row.querySelector(".key");
      if (k && k.textContent === key) row.classList.add("selected");
    });
    log("info", `выбран узел key=${key}`);
  }

  // ---------- state ---------------------------------------------------------
  function renderState(data) {
    if (data && typeof data === "object") {
      stateEl.textContent = JSON.stringify(data, null, 2);
    } else {
      stateEl.textContent = String(data ?? "—");
    }
    const keys = data && typeof data === "object" ? Object.keys(data).length : 0;
    $("state-meta").textContent = keys ? `${keys} полей` : "—";
  }

  // ---------- API helpers ---------------------------------------------------
  async function apiTree() {
    const r = await fetch("/api/tree", { cache: "no-store" });
    return r.json();
  }
  async function apiState() {
    const r = await fetch("/api/state", { cache: "no-store" });
    return r.json();
  }
  async function apiCommand(cmd, args) {
    const r = await fetch("/api/command", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd, args: args ?? null }),
    });
    return r.json();
  }

  async function refreshAll() {
    try {
      const [t, s] = await Promise.all([apiTree(), apiState()]);
      // HTTP-контракт: {ok: true, result: <object>} | {ok: false, error: "<msg>"}.
      const tErr = t && !t.ok && t.error;
      const sErr = s && !s.ok && s.error;
      if (tErr || sErr) {
        setBadge("off", "Приложение не запущено");
        renderTree(null);
        renderState(null);
        log("err", `devtools недоступен: ${tErr || sErr}`);
      } else {
        setBadge("on", "Соединено");
        renderTree(t.result);
        renderState(s.result);
        lastTreeAt = Date.now();
        lastStateAt = Date.now();
        log("ok", "tree+state получены");
      }
    } catch (e) {
      setBadge("off", "Ошибка сети");
      log("err", String(e));
    }
  }

  // ---------- кнопки --------------------------------------------------------
  btnRefresh.addEventListener("click", refreshAll);
  btnClearLog.addEventListener("click", () => { logEl.innerHTML = ""; });

  btnClick.addEventListener("click", async () => {
    if (!selectedKey) return;
    log("info", `click → key=${selectedKey}`);
    const res = await apiCommand("click", { key: selectedKey });
    if (res.ok) {
      log("ok", `клик выполнен (${JSON.stringify(res.result ?? {}).slice(0, 120)})`);
      await refreshAll();
    } else {
      log("err", `ошибка клика: ${JSON.stringify(res)}`);
    }
  });

  btnShot.addEventListener("click", async () => {
    log("info", "запрашиваю скриншот (SHOT)…");
    const res = await apiCommand("shot");
    if (!res.ok) {
      log("err", `ошибка SHOT: ${JSON.stringify(res)}`);
      return;
    }
    const raw = typeof res.result === "string" ? res.result : "";
    // Формат от TCP: base64:<png>
    const m = raw.match(/^base64:(.+)$/s);
    if (!m) {
      log("err", `неожиданный ответ SHOT: ${raw.slice(0, 120)}`);
      return;
    }
    const b64 = m[1].trim();
    const a = document.createElement("a");
    a.href = `data:image/png;base64,${b64}`;
    a.download = `light-shots-${Date.now()}.png`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    log("ok", "скриншот сохранён");
  });

  // ---------- SSE -----------------------------------------------------------
  function connectEvents() {
    const es = new EventSource("/api/events");
    es.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg.kind === "hello") {
          setBadge("on", "Соединено");
        } else if (msg.kind === "tree") handleEvent("tree", msg.data);
        else if (msg.kind === "state") handleEvent("state", msg.data);
        else if (msg.kind === "error") {
          setBadge("off", "Ошибка");
          log("err", msg.data && msg.data.message ? msg.data.message : "unknown error");
        }
      } catch (e) {
        // не JSON — игнорируем
      }
    };
    es.onerror = () => {
      // EventSource сам переподключается; обновляем бейдж на "ожидание".
      setBadge("waiting", "Переподключение…");
    };
  }

  function handleEvent(kind, data) {
    if (data && data.error) {
      setBadge("off", "Приложение не запущено");
      return;
    }
    setBadge("on", "Соединено");
    if (kind === "tree") {
      renderTree(data);
      lastTreeAt = Date.now();
    } else if (kind === "state") {
      renderState(data);
      lastStateAt = Date.now();
    }
  }

  // ---------- init ----------------------------------------------------------
  connectEvents();
  refreshAll();
  setInterval(() => {
    // Если SSE молчит >10 с — принудительный refresh (страховка).
    const now = Date.now();
    if (!lastTreeAt || now - lastTreeAt > 10_000) refreshAll();
  }, 5000);
})();
