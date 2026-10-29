const { invoke } = window.__TAURI__.core;

export async function render(container) {
  container.innerHTML = `
    <div class="scan-header">
      <h2 style="margin:0">局域网扫描</h2>
      <span style="flex:1"></span>
      <button id="btn-refresh">刷新</button>
      <button id="btn-pause">暂停</button>
      <div class="scan-max-row">
        <label class="scan-label">同IP上限:</label>
        <input id="max-per-ip" type="number" min="0" value="3" class="scan-input-sm" title="0 = 不限制" />
      </div>
    </div>
    
    <div class="scan-table">
      <div class="scan-row-header">
        <div class="col-icon">图标<span class="resize-handle" data-resize="icon" title="拖动调整列宽"></span></div>
        <div class="col-motd">MOTD / Radmin 名称</div>
        <div class="col-count">次数<span class="resize-handle" data-resize="count" title="拖动调整列宽"></span></div>
        <div class="col-meta">IP:端口<span class="resize-handle" data-resize="meta" title="拖动调整列宽"></span></div>
      </div>
      <div class="scan-list" id="scan-results">
        <div class="scan-empty">等待广播包...</div>
      </div>
    </div>
  `;

  const $ = (id) => document.getElementById(id);
  const listEl = $("scan-results");
  const maxInput = $("max-per-ip");
  const btnPause = $("btn-pause");
  const btnRefresh = $("btn-refresh");
  let paused = false;

  const MC_COLORS = {
    "0":"#000000","1":"#0000AA","2":"#00AA00","3":"#00AAAA",
    "4":"#AA0000","5":"#AA00AA","6":"#FFAA00","7":"#AAAAAA",
    "8":"#555555","9":"#5555FF","a":"#55FF55","b":"#55FFFF",
    "c":"#FF5555","d":"#FF55FF","e":"#FFFF55","f":"#FFFFFF"
  };

  function escapeHtml(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }

  function renderMotd(motd) {
    let color = "#FFFFFF", out = "", i = 0;
    while (i < motd.length) {
      if (motd[i] === "§" && i + 1 < motd.length) {
        const code = motd[i + 1].toLowerCase();
        if (MC_COLORS[code]) { color = MC_COLORS[code]; i += 2; continue; }
        i += 2; continue;
      }
      const c = motd[i] === " " ? "&#160;" : escapeHtml(motd[i]);
      out += `<span style="color:${color}">${c}</span>`;
      i++;
    }
    return out;
  }

  let lastSig = "";
  let nameByIp = new Map();

  // 把广播来源 IP 映射到 Radmin 成员名称（IP -> alias）
  async function refreshNameMap() {
    try {
      const snap = await invoke("get_radmin_snapshot");
      const map = new Map();
      for (const net of snap.networks || []) {
        for (const peer of net.peers || []) {
          if (!peer.ipv4 || !peer.name) continue;
          if (!map.has(peer.ipv4)) map.set(peer.ipv4, peer.name);
        }
      }
      nameByIp = map;
      lastSig = "";
    } catch (e) {
      console.warn("scan name map failed:", e);
    }
  }

  async function refresh() {
    try {
      const results = await invoke("scan_get_results");
      const sig = JSON.stringify(
        results.map(r => [r.motd, r.ad, r.count, r.time, r.icon, r.ip, nameByIp.get(r.ip) || ""]),
      );
      if (sig === lastSig) return;
      lastSig = sig;
      
      if (!results || results.length === 0) {
        if (listEl) listEl.innerHTML = '<div class="scan-empty">等待广播包...</div>';
        return;
      }
      
      const sorted = [...results].sort((a, b) => (b.time || 0) - (a.time || 0));
      const html = sorted.map(r => {
        const motdHtml = renderMotd(r.motd || "");
        const radminName = nameByIp.get(r.ip) || "";
        const nameHtml = radminName
          ? `<span class="motd-name">${escapeHtml(radminName)}</span>`
          : '<span class="motd-name is-unknown">未知设备</span>';
        const plainMotd = String(r.motd || "").replace(/§./g, "");
        const tip = `${radminName || "未知设备"}\n${plainMotd}`;
        let imgTag = '<div class="icon-placeholder"></div>';
        if (isSafeImageData(r.icon)) {
          imgTag = `<img src="${r.icon}" class="server-icon" alt="" />`;
        }
        
        return `
          <div class="scan-row">
            <div class="col-icon">${imgTag}</div>
            <div class="col-motd" title="${escapeHtml(tip)}">
              <div class="motd-line">${motdHtml}</div>
              <div class="motd-sub">${nameHtml}</div>
            </div>
            <div class="col-count">×${r.count}</div>
            <div class="col-meta">${escapeHtml(r.ip || "")}:${escapeHtml(r.ad || "")}</div>
          </div>
        `;
      }).join("");
      
      if (listEl) listEl.innerHTML = html;
    } catch (e) { console.warn("scan_get_results failed:", e); }
  }

  function isSafeImageData(value) {
    return /^data:image\/(?:png|jpeg|gif|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(value || "");
  }

  const scanHeader = container.querySelector(".scan-row-header");
  scanHeader.querySelectorAll(".resize-handle").forEach(handle => {
    handle.addEventListener("pointerdown", event => {
      event.preventDefault();
      const column = handle.dataset.resize;
      const columnEl = handle.parentElement;
      const startX = event.clientX;
      const startWidth = columnEl.getBoundingClientRect().width;
      const minWidth = column === "icon" ? 32 : column === "count" ? 40 : 80;
      handle.setPointerCapture(event.pointerId);

      const resize = moveEvent => {
        const width = Math.max(minWidth, startWidth + moveEvent.clientX - startX);
        container.style.setProperty(`--scan-${column}-width`, `${width}px`);
      };
      const finish = () => {
        handle.removeEventListener("pointermove", resize);
        handle.removeEventListener("pointerup", finish);
        handle.removeEventListener("pointercancel", finish);
      };
      handle.addEventListener("pointermove", resize);
      handle.addEventListener("pointerup", finish);
      handle.addEventListener("pointercancel", finish);
    });
  });

  if (maxInput) {
    try {
      maxInput.value = await invoke("scan_get_max_per_ip");
    } catch (e) {
      console.error("scan_get_max_per_ip failed:", e);
    }
    maxInput.addEventListener("change", async () => {
      const limit = Math.max(0, Number.parseInt(maxInput.value, 10) || 0);
      maxInput.value = limit;
      try {
        await invoke("scan_set_max_per_ip", { limit });
      } catch (e) {
        console.error("scan_set_max_per_ip failed:", e);
      }
    });
  }

  try {
    await invoke("scan_start");
  } catch (e) {
    console.error("scan_start failed:", e);
  }
  
  if (btnPause) {
    btnPause.addEventListener("click", async () => {
      try {
        if (!paused) {
          await invoke("scan_stop");
          paused = true;
          btnPause.textContent = "继续";
          btnPause.classList.add("paused");
        } else {
          await invoke("scan_start");
          paused = false;
          btnPause.textContent = "暂停";
          btnPause.classList.remove("paused");
        }
      } catch (e) {
        console.error("scan pause toggle failed:", e);
      }
    });
  }

  await refreshNameMap();
  btnRefresh.addEventListener("click", async () => {
    await refreshNameMap();
    await refresh();
  });
  const nameTimer = setInterval(() => {
    if (!container.isConnected) {
      clearInterval(nameTimer);
      return;
    }
    refreshNameMap();
  }, 15000);
  const refreshTimer = setInterval(() => {
    if (!container.isConnected) {
      clearInterval(refreshTimer);
      clearInterval(nameTimer);
      return;
    }
    refresh();
  }, 500);
  await refresh();
}