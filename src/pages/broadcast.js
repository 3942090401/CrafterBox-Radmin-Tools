const { invoke } = window.__TAURI__.core;

function isAlive() {
  return document.getElementById("flow-chart") !== null;
}

export function render(container) {
  const cfg = window.__radminConfig.get();
  const defaultMotd = cfg.broadcast_motd;
  const defaultCount = cfg.broadcast_count;
  const defaultPrefix = cfg.broadcast_prefix;
  const portPad = cfg.port_pad;

  container.innerHTML = `
    <h2>广播设置</h2>
    <div class="broadcast-top">
      <div class="row">
        <label>MOTD:</label>
        <div class="motd-wrap">
          <div id="motd-preview" class="motd-preview"></div>
          <input id="motd-input" class="motd-real-input" placeholder="{num}=编号" />
        </div>
      </div>
      <div class="row">
        <label>数量:</label>
        <input id="count-input" type="number" value="${defaultCount}" style="flex:0 0 100px" />
        <label>端口前缀:</label>
        <div class="motd-wrap" style="flex:1">
          <div id="prefix-preview" class="motd-preview"></div>
          <input id="prefix-input" class="motd-real-input" placeholder="留空则只有数字" />
        </div>
        <label class="checkbox" style="margin-left:10px">
          <input type="checkbox" id="port-pad" ${portPad ? "checked" : ""} /> 端口补零
        </label>
      </div>
    </div>

    <div class="broadcast-layout">
      <div class="broadcast-left">
        <div class="palette">
          <div class="palette-row" id="palette-row1"></div>
          <div class="palette-row" id="palette-row2"></div>
          <div class="palette-row" id="palette-row3"></div>
        </div>
      </div>
      <div class="broadcast-right">
        <div class="ctrl-btns">
          <button id="btn-start">开始</button>
          <button id="btn-pause" disabled>暂停</button>
          <button id="btn-stop" disabled>停止</button>
        </div>
        <div class="ctrl-status">状态: <span id="status" class="status-off">未运行</span></div>
      </div>
    </div>

    <div class="chart-wrap">
      <h2>流量统计</h2>
      <div class="chart-stats">
        <span>发送包数: <b id="stat-packets">0</b></span>
        <span>速率: <b id="stat-rate">0</b> pkt/s</span>
        <span>峰值: <b id="stat-peak">0</b> pkt/s</span>
      </div>
      <canvas class="flow-chart" id="flow-chart"></canvas>
    </div>
  `;

  const $ = (id) => document.getElementById(id);
  const COLORS_ROW1 = {
    "§0":"#000000","§1":"#0000AA","§2":"#00AA00","§3":"#00AAAA",
    "§4":"#AA0000","§5":"#AA00AA","§6":"#FFAA00","§7":"#AAAAAA"
  };
  const COLORS_ROW2 = {
    "§8":"#555555","§9":"#5555FF","§a":"#55FF55","§b":"#55FFFF",
    "§c":"#FF5555","§d":"#FF55FF","§e":"#FFFF55","§f":"#FFFFFF"
  };
  const MC_COLORS = {
    "0":"#000000","1":"#0000AA","2":"#00AA00","3":"#00AAAA",
    "4":"#AA0000","5":"#AA00AA","6":"#FFAA00","7":"#AAAAAA",
    "8":"#555555","9":"#5555FF","a":"#55FF55","b":"#55FFFF",
    "c":"#FF5555","d":"#FF55FF","e":"#FFFF55","f":"#FFFFFF"
  };
  const OBFUSCATED_CHARS = "1234567890!@#$%^&*()-=[];',./QWERTYUIOPASDFGHJKLZXCVBNM<>:\"?{}";

  const motdInput = $("motd-input");
  const prefixInput = $("prefix-input");
  const countInput = $("count-input");
  const portPadCheck = $("port-pad");

  motdInput.value = defaultMotd;
  prefixInput.value = defaultPrefix;

  let lastFocusedInput = motdInput;

  function saveBroadcastConfig() {
    if (!window.__radminConfig) return;
    window.__radminConfig.save({
      broadcast_motd: motdInput.value,
      broadcast_count: countInput.value,
      broadcast_prefix: prefixInput.value,
      port_pad: portPadCheck.checked
    });
  }

  motdInput.addEventListener("input", saveBroadcastConfig);
  countInput.addEventListener("input", saveBroadcastConfig);
  prefixInput.addEventListener("input", saveBroadcastConfig);
  portPadCheck.addEventListener("change", saveBroadcastConfig);

  function insertCode(code) {
    const target = lastFocusedInput || motdInput;
    if (!target) return;
    const start = target.selectionStart ?? target.value.length;
    target.value = target.value.slice(0, start) + code + target.value.slice(start);
    target.focus();
    target.selectionStart = target.selectionEnd = start + 2;
    updatePreview(target);
    saveBroadcastConfig();
  }

  function buildSwatch(code, color) {
    const s = document.createElement("div");
    s.className = "swatch";
    s.style.background = color;
    s.dataset.code = code;
    s.onclick = () => insertCode(code);
    return s;
  }

  const row1 = $("palette-row1"), row2 = $("palette-row2"), row3 = $("palette-row3");
  Object.entries(COLORS_ROW1).forEach(([c, col]) => row1.appendChild(buildSwatch(c, col)));
  Object.entries(COLORS_ROW2).forEach(([c, col]) => row2.appendChild(buildSwatch(c, col)));

  function buildFormatBtn(code, label, title, extraStyle) {
    const b = document.createElement("div");
    b.className = "swatch format";
    b.dataset.code = code;
    b.textContent = label;
    b.title = title;
    Object.assign(b.style, {
      background: "rgba(60, 60, 60, 0.9)", color: "#fff", fontSize: "14px",
      display: "flex", alignItems: "center", justifyContent: "center", fontWeight: "bold",
    }, extraStyle || {});
    b.onclick = () => insertCode(code);
    return b;
  }

  row3.appendChild(buildFormatBtn("§l", "B", "粗体"));
  row3.appendChild(buildFormatBtn("§o", "I", "斜体", { fontStyle: "italic" }));
  row3.appendChild(buildFormatBtn("§n", "U", "下划线", { textDecoration: "underline" }));
  row3.appendChild(buildFormatBtn("§m", "S", "删除线", { textDecoration: "line-through" }));
  const kBtn = buildFormatBtn("§k", "?", "乱码");
  setInterval(() => {
    if (!kBtn || !kBtn.parentNode) return;
    kBtn.textContent = OBFUSCATED_CHARS[Math.floor(Math.random() * OBFUSCATED_CHARS.length)];
  }, 50);
  row3.appendChild(kBtn);
  row3.appendChild(buildFormatBtn("§r", "×", "重置", { color: "#ff6666" }));

  function escapeHtml(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }

  function renderPreview(text, focused) {
    let color = "#FFFFFF", bold = false, italic = false, underline = false, strike = false, obfuscated = false;
    let out = "", i = 0;
    while (i < text.length) {
      if (text[i] === "§" && i + 1 < text.length) {
        const code = text[i + 1].toLowerCase();
        if (focused) out += `<span style="color:#888">${escapeHtml(text[i] + text[i+1])}</span>`;
        if (MC_COLORS[code]) { color = MC_COLORS[code]; bold = italic = underline = strike = false; obfuscated = false; }
        else if (code === "r") { color = "#FFFFFF"; bold = italic = underline = strike = false; obfuscated = false; }
        else if (code === "l") bold = true;
        else if (code === "o") italic = true;
        else if (code === "n") underline = true;
        else if (code === "m") strike = true;
        else if (code === "k") obfuscated = true;
        i += 2; continue;
      }
      let rawChar = text[i], c;
      if (rawChar === " ") c = "&#160;";
      else if (obfuscated) c = escapeHtml(OBFUSCATED_CHARS[Math.floor(Math.random() * OBFUSCATED_CHARS.length)]);
      else c = escapeHtml(rawChar);
      const styles = [`color:${color}`];
      if (bold) styles.push("font-weight:bold");
      if (italic) styles.push("font-style:italic");
      const deco = [];
      if (underline) deco.push("underline");
      if (strike) deco.push("line-through");
      if (deco.length) styles.push(`text-decoration:${deco.join(" ")}`);
      out += `<span style="${styles.join(";")}">${c}</span>`;
      i++;
    }
    return out;
  }

  function updatePreview(input) {
    const id = input ? input.id : "motd-input";
    const prevId = id === "motd-input" ? "motd-preview" : "prefix-preview";
    const el = $(prevId);
    const src = input || $(id);
    if (!el || !src) return;
    const focused = document.activeElement === src;
    el.innerHTML = renderPreview(src.value, focused);
  }

  setInterval(() => {
    if (!isAlive()) return;
    const hasK = motdInput.value.toLowerCase().includes("§k") || prefixInput.value.toLowerCase().includes("§k");
    if (hasK) { updatePreview(motdInput); updatePreview(prefixInput); }
  }, 50);

  motdInput.addEventListener("focus", () => { lastFocusedInput = motdInput; updatePreview(motdInput); });
  prefixInput.addEventListener("focus", () => { lastFocusedInput = prefixInput; updatePreview(prefixInput); });
  ["input","keyup","click"].forEach(evt => {
    motdInput.addEventListener(evt, () => updatePreview(motdInput));
    prefixInput.addEventListener(evt, () => updatePreview(prefixInput));
  });
  motdInput.addEventListener("blur", () => updatePreview(motdInput));
  prefixInput.addEventListener("blur", () => updatePreview(prefixInput));
  updatePreview(motdInput);
  updatePreview(prefixInput);

  // 折线图
  const canvas = $("flow-chart");
  const ctx = canvas.getContext("2d");
  const MAX_POINTS = 60;
  const data = new Array(MAX_POINTS).fill(0);
  let totalPackets = 0, peakRate = 0;

  function drawChart() {
    const w = canvas.width, h = canvas.height;
    if (w <= 0 || h <= 0) return;
    ctx.clearRect(0, 0, w, h);
    ctx.strokeStyle = "rgba(255,255,255,0.08)";
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
      const y = (h / 4) * i;
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }
    const maxVal = Math.max(...data, 10);
    ctx.strokeStyle = "#00ff88";
    ctx.lineWidth = 2;
    ctx.beginPath();
    data.forEach((v, i) => {
      const x = (i / (MAX_POINTS - 1)) * w;
      const y = h - (v / maxVal) * (h - 10) - 5;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.lineTo(w, h); ctx.lineTo(0, h);
    ctx.closePath();
    ctx.fillStyle = "rgba(0, 255, 136, 0.12)";
    ctx.fill();
  }
  drawChart();

  let lastW = 0, lastH = 0;
  const resizeCanvas = () => {
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const w = Math.floor(rect.width), h = Math.floor(rect.height);
    if (w <= 0 || h <= 0) return;
    if (w === lastW && h === lastH) return;
    lastW = w; lastH = h;
    canvas.width = w; canvas.height = h;
    drawChart();
  };
  if (canvas.parentElement) {
    const ro = new ResizeObserver(resizeCanvas);
    ro.observe(canvas.parentElement);
    setTimeout(resizeCanvas, 100);
  }

  let pendingRate = 0;
  setInterval(() => {
    if (!isAlive()) return;
    const sPackets = $("stat-packets"), sRate = $("stat-rate"), sPeak = $("stat-peak");
    if (!sPackets || !sRate || !sPeak) return;
    data.shift(); data.push(pendingRate);
    if (pendingRate > peakRate) peakRate = pendingRate;
    sPackets.textContent = totalPackets;
    sRate.textContent = pendingRate;
    sPeak.textContent = peakRate;
    drawChart();
    pendingRate = 0;
  }, 1000);

  const btnStart = $("btn-start"), btnPause = $("btn-pause"), btnStop = $("btn-stop");
  const statusEl = $("status");
  let running = false, paused = false;
  function setStatus(text, cls) { statusEl.textContent = text; statusEl.className = cls; }

  btnStart.addEventListener("click", async () => {
    const count = parseInt(countInput.value) || 10;
    const motd = motdInput.value.trim() || "服务器{num}";
    const prefix = prefixInput.value.trim();
    try {
      await invoke("start_broadcast_cmd", { count, motd, prefix });
      running = true; paused = false;
      setStatus("正在运行", "status-on");
      btnStart.disabled = true; btnPause.disabled = false; btnStop.disabled = false;
      countInput.disabled = true; motdInput.disabled = true; prefixInput.disabled = true;
    } catch (e) { alert("启动失败: " + e); }
  });

  btnPause.addEventListener("click", async () => {
    if (!running) return;
    if (paused) {
      await invoke("resume_broadcast_cmd");
      paused = false; btnPause.textContent = "暂停";
      setStatus("正在运行", "status-on");
    } else {
      await invoke("pause_broadcast_cmd");
      paused = true; btnPause.textContent = "继续";
      setStatus("已暂停", "status-paused");
    }
  });

  btnStop.addEventListener("click", async () => {
    await invoke("stop_broadcast_cmd");
    running = false; paused = false;
    setStatus("未运行", "status-off");
    btnStart.disabled = false; btnPause.disabled = true; btnPause.textContent = "暂停";
    btnStop.disabled = true;
    countInput.disabled = false; motdInput.disabled = false; prefixInput.disabled = false;
  });

  setInterval(async () => {
    if (!running || !isAlive()) return;
    try {
      const stats = await invoke("get_broadcast_stats");
      totalPackets = stats[0]; pendingRate = stats[1];
    } catch (e) {}
  }, 1000);
}