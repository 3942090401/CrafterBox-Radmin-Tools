import "./lan-radmin.css";
import { toast } from "../ui.js";

const { invoke } = window.__TAURI__.core;

const REFRESH_MS = 20000;
const PING_LIMIT = 160;
const FILTER_DELAY = 60;

const COLUMNS = ["名称", "RID", "虚拟IP4", "虚拟IP6", "公网地址", "Ping", "通道", "状态"];

export async function render(container) {
  container.classList.add("rm-page");
  container.innerHTML = `
    <div class="rm-head">
      <div class="rm-head-left">
        <div class="rm-title">
          <span>网络管理</span>
          <span class="rm-dot" id="rm-dot"></span>
          <span class="rm-count" id="rm-count">正在读取</span>
        </div>
      </div>
      <div class="rm-self">
        <span>本机 <b id="rm-self-alias">读取中…</b></span>
        <span>虚拟IP <b id="rm-self-ip">—</b></span>
        <button id="rm-refresh">刷新</button>
      </div>
    </div>

    <div class="rm-picker">
      <div class="rm-picker-head">
        <span class="rm-picker-label">网络</span>
        <span class="rm-pick-count" id="rm-pick-count">—</span>
        <span class="rm-picker-spacer"></span>
        <button class="rm-selall" id="rm-select-all">全选</button>
        <button class="rm-join-btn" id="rm-join-toggle">加入网络</button>
      </div>
      <div class="rm-nets" id="rm-nets"></div>
      <div class="rm-join-form" id="rm-join-form" hidden>
        <input id="rm-join-name" type="text" autocomplete="off" spellcheck="false" placeholder="网络名（需与对方完全一致，含空格）" />
        <input id="rm-join-pass" type="text" autocomplete="off" spellcheck="false" placeholder="访问码（可选）" />
        <input id="rm-join-flag" class="rm-join-flag" type="text" autocomplete="off" spellcheck="false" value="0" title="标志位 0x033F，正常加网填 0" />
        <button id="rm-join-submit">加入</button>
        <button id="rm-join-cancel">取消</button>
      </div>
    </div>

    <div class="rm-panel rm-grow">
      <div class="rm-panel-head">
        <h3 id="rm-net-name">—</h3>
        <span class="rm-badge" id="rm-net-online">—</span>
        <span class="rm-panel-sub" id="rm-net-meta">GUI 管道实时</span>
      </div>
      <div class="rm-toolbar">
        <input class="rm-search" id="rm-search" type="text" autocomplete="off" spellcheck="false"
               placeholder="搜索名称 / RID / 虚拟 IP / 公网地址" />
        <label class="rm-check"><input type="checkbox" id="rm-online-only" />只看在线</label>
        <button class="rm-ping-btn" id="rm-ping">测延迟</button>
      </div>
      <div class="rm-table" id="rm-table">
        <div class="rm-row rm-head-row">
          ${COLUMNS.map((label) => `<span>${label}</span>`).join("")}
        </div>
        <div id="rm-rows"></div>
      </div>
      <div class="rm-note">
        <span id="rm-note-count">—</span>
      </div>
    </div>

  `;

  const $ = (id) => container.querySelector(`#${id}`);
  const dot = $("rm-dot");
  const countEl = $("rm-count");
  const selfAlias = $("rm-self-alias");
  const selfIp = $("rm-self-ip");
  const refreshBtn = $("rm-refresh");
  const netsEl = $("rm-nets");
  const pickCount = $("rm-pick-count");
  const selAllBtn = $("rm-select-all");
  const joinToggle = $("rm-join-toggle");
  const joinForm = $("rm-join-form");
  const joinName = $("rm-join-name");
  const joinPass = $("rm-join-pass");
  const joinFlag = $("rm-join-flag");
  const searchEl = $("rm-search");
  const onlineOnly = $("rm-online-only");
  const netName = $("rm-net-name");
  const netOnline = $("rm-net-online");
  const netMeta = $("rm-net-meta");
  const rowsEl = $("rm-rows");
  const noteCount = $("rm-note-count");

  let snapshot = null;
  let selected = new Set();
  let joinOpen = false;
  let disposed = false;
  let pingPending = false;
  let pinged = false;
  let filterTimer = 0;
  const pingMap = new Map();

  /* ---------- 网络勾选 ---------- */

  const selectedNetworks = () =>
    snapshot ? snapshot.networks.filter((network) => selected.has(network.name)) : [];

  function updatePickMeta() {
    const total = snapshot?.networks.length || 0;
    const on = selected.size;
    if (total === 0) {
      pickCount.textContent = "0";
      selAllBtn.textContent = "全选";
      selAllBtn.disabled = true;
      return;
    }
    selAllBtn.disabled = false;
    pickCount.textContent = `${on}/${total}`;
    selAllBtn.textContent = on === total ? "取消全选" : "全选";
  }

  function renderNets() {
    netsEl.replaceChildren();
    const networks = snapshot?.networks || [];
    if (networks.length === 0) {
      const empty = document.createElement("span");
      empty.className = "rm-nets-empty";
      empty.textContent = "未加入任何网络";
      netsEl.appendChild(empty);
      updatePickMeta();
      return;
    }
    const fragment = document.createDocumentFragment();
    for (const network of networks) {
      const online = network.peers.filter((peer) => peer.online).length;
      const label = document.createElement("label");
      label.className = "rm-net";
      label.classList.toggle("is-on", selected.has(network.name));
      label.title = network.guid ? `GUID ${network.guid}` : network.name;

      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = selected.has(network.name);
      box.addEventListener("change", () => {
        if (box.checked) selected.add(network.name);
        else selected.delete(network.name);
        label.classList.toggle("is-on", box.checked);
        updatePickMeta();
        onSelectionChanged();
      });

      const name = document.createElement("span");
      name.className = "rm-net-name";
      name.textContent = network.name;
      const badge = document.createElement("span");
      badge.className = "rm-net-count";
      badge.textContent = `${online}/${network.peers.length}`;

      label.append(box, name, badge);
      fragment.appendChild(label);
    }
    netsEl.appendChild(fragment);
    updatePickMeta();
  }

  function onSelectionChanged() {
    pinged = false;
    pingMap.clear();
    renderPanel();
  }

  /* ---------- 成员表 ---------- */

  function collectPeers() {
    const seen = new Set();
    const out = [];
    for (const network of selectedNetworks()) {
      for (const peer of network.peers) {
        const key = `${peer.rid || ""}|${peer.ipv4 || ""}|${peer.name || ""}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push(peer);
      }
    }
    return out;
  }

  function renderPanel() {
    const networks = selectedNetworks();
    const peers = collectPeers();
    const totalPeers = peers.length;
    const onlinePeers = peers.filter((peer) => peer.online).length;

    if (networks.length === 0) {
      netName.textContent = "未选择网络";
      netName.title = "";
      netOnline.textContent = "—";
      netOnline.classList.add("is-muted");
      netMeta.textContent = "勾选上方网络后显示成员";
    } else if (networks.length === 1) {
      const net = networks[0];
      netName.textContent = net.name;
      netName.title = net.guid ? `${net.name}\nGUID ${net.guid}` : net.name;
      netOnline.textContent = `在线 ${onlinePeers}/${totalPeers}`;
      netOnline.classList.remove("is-muted");
      const parts = [];
      if (net.access_code) parts.push(`访问码 ${net.access_code}`);
      if (net.enabled === false) parts.push("已停用");
      netMeta.textContent = parts.join(" · ");
    } else {
      const all = snapshot && networks.length === snapshot.networks.length;
      netName.textContent = all ? "全部网络" : `已选 ${networks.length} 个网络`;
      netName.title = networks.map((net) => net.name).join("\n");
      netOnline.textContent = `在线 ${onlinePeers}/${totalPeers}`;
      netOnline.classList.remove("is-muted");
      netMeta.textContent = "";
    }
    renderRows(peers);
  }

  function pingCellText(peer) {
    if (!peer.online) return "—";
    const value = peer.ipv4 ? pingMap.get(peer.ipv4) : undefined;
    if (value != null) return `${value} ms`;
    if (pingPending) return "…";
    return pinged ? "超时" : "—";
  }

  function pingClass(text) {
    if (!text.endsWith("ms")) return "";
    const value = Number.parseInt(text, 10);
    if (value <= 80) return "rm-ping-good";
    if (value <= 200) return "rm-ping-mid";
    return "rm-ping-bad";
  }

  async function copyCell(cell, value) {
    try {
      await navigator.clipboard.writeText(value);
      cell.classList.add("is-copied");
      window.setTimeout(() => cell.classList.remove("is-copied"), 620);
      toast(`已复制 ${value}`, "success", 1200);
    } catch (error) {
      toast(`复制失败：${error}`, "error", 2200);
    }
  }

  let rowEntries = [];

  function renderRows(all = collectPeers()) {
    rowsEl.replaceChildren();
    rowEntries = [];

    const empty = document.createElement("div");
    empty.className = "rm-empty";
    empty.hidden = true;
    rowsEl.appendChild(empty);

    const fragment = document.createDocumentFragment();
    for (const peer of all) {
      const row = document.createElement("div");
      row.className = "rm-row rm-body-row";
      row.classList.toggle("is-offline", !peer.online);
      row.title = [peer.name, `${peer.ipv4 || "—"}/${peer.ipv4_prefix ?? 8}`, peer.session_remote]
        .filter(Boolean)
        .join("\n");

      const pingText = pingCellText(peer);
      const cells = [
        { text: peer.name || "—", cls: "rm-cell-name" },
        { text: peer.rid || "—" },
        { text: peer.ipv4 || "—", prefix: peer.ipv4 ? `/${peer.ipv4_prefix ?? 8}` : null },
        { text: peer.ipv6 || "—" },
        { text: peer.public_address || "—" },
        { text: pingText, cls: pingClass(pingText) },
        { text: peer.channel || "—" },
        { text: peer.online ? "在线" : "离线", cls: peer.online ? "rm-online" : "rm-offline" },
      ];

      for (const def of cells) {
        const cell = document.createElement("span");
        cell.className = "rm-cell";
        if (def.cls) cell.classList.add(def.cls);
        cell.textContent = def.text;
        if (def.prefix) {
          const prefix = document.createElement("i");
          prefix.className = "rm-prefix";
          prefix.textContent = def.prefix;
          cell.appendChild(prefix);
        }
        if (def.text && def.text !== "—") {
          const value = def.prefix ? `${def.text}${def.prefix}` : def.text;
          cell.title = `点击复制：${value}`;
          cell.addEventListener("click", (event) => {
            event.stopPropagation();
            void copyCell(cell, value);
          });
        }
        row.appendChild(cell);
      }

      rowEntries.push({
        node: row,
        online: Boolean(peer.online),
        key: [peer.name, peer.rid, peer.ipv4, peer.ipv6, peer.public_address]
          .filter(Boolean)
          .join(" ")
          .toLocaleLowerCase(),
      });
      fragment.appendChild(row);
    }
    rowsEl.appendChild(fragment);
    applyFilter(all.length);
  }

  // 过滤只切换行显隐，不重建 DOM，所以上万行也不会打字卡顿
  function applyFilter(total = rowEntries.length) {
    const query = searchEl.value.trim().toLocaleLowerCase();
    const onlyOnline = onlineOnly.checked;
    let visible = 0;
    for (const entry of rowEntries) {
      const hit = (!onlyOnline || entry.online) && (!query || entry.key.includes(query));
      entry.node.classList.toggle("is-hidden", !hit);
      if (hit) visible += 1;
    }

    const emptyEl = rowsEl.querySelector(".rm-empty");
    if (emptyEl) {
      emptyEl.hidden = visible > 0;
      emptyEl.textContent =
        total === 0
          ? "没有可显示的成员"
          : query
            ? "没有符合条件的成员"
            : "没有在线成员";
    }

    const noteParts = [`共 ${total} 条成员`];
    if (visible !== total) noteParts.push(`筛选后 ${visible} 条`);
    noteCount.textContent = noteParts.join(" · ");
  }

  function scheduleFilter() {
    if (filterTimer) return;
    filterTimer = window.setTimeout(() => {
      filterTimer = 0;
      applyFilter();
    }, FILTER_DELAY);
  }

  async function runPing(force) {
    if (pingPending) return;
    if (!force && pinged) return;
    const hosts = collectPeers()
      .filter((peer) => peer.online && peer.ipv4)
      .map((peer) => peer.ipv4)
      .filter((ip, index, list) => list.indexOf(ip) === index)
      .slice(0, PING_LIMIT);
    if (hosts.length === 0) return;

    pingPending = true;
    renderRows();
    try {
      const result = await invoke("ping_radmin_peers", { hosts, timeoutMs: 1200 });
      for (const [host, ms] of Object.entries(result)) pingMap.set(host, ms);
      pinged = true;
    } catch (error) {
      toast(`延迟探测失败：${error}`, "error", 2400);
    } finally {
      pingPending = false;
      renderRows();
    }
  }

  async function loadSnapshot({ auto = false, ping = false } = {}) {
    refreshBtn.disabled = true;
    try {
      const data = await invoke("get_radmin_snapshot");
      snapshot = data;
      dot.classList.remove("is-error");
      countEl.textContent = `已加入 ${data.networks.length} 个网络`;
      selfAlias.textContent = data.self_node.alias || "—";
      selfIp.textContent = data.self_node.ipv4 || "—";

      const names = new Set(data.networks.map((network) => network.name));
      for (const name of [...selected]) {
        if (!names.has(name)) selected.delete(name);
      }
      if (selected.size === 0) {
        for (const name of names) selected.add(name);
        pinged = false;
        pingMap.clear();
      }

      renderNets();
      renderPanel();
      if (ping) void runPing(true);
    } catch (error) {
      dot.classList.add("is-error");
      countEl.textContent = "管道不可用";
      if (!auto) toast(`读取失败：${error}`, "error", 2600);
    } finally {
      refreshBtn.disabled = false;
    }
  }

  function toggleJoin(show) {
    joinOpen = show;
    joinForm.hidden = !show;
    netsEl.hidden = show;
    selAllBtn.hidden = show;
    joinToggle.hidden = show;
    if (show) joinName.focus();
  }

  async function submitJoin() {
    const name = joinName.value.trim();
    if (!name) {
      toast("请填写网络名", "error", 2000);
      joinName.focus();
      return;
    }
    const password = joinPass.value;
    const flag = Number.parseInt(joinFlag.value.trim() || "0", 10);
    const submit = $("rm-join-submit");
    submit.disabled = true;
    submit.textContent = "加入中…";
    try {
      const reply = await invoke("join_radmin_network", {
        name,
        password,
        flag: Number.isFinite(flag) ? flag >>> 0 : 0,
      });
      if (reply.code === 0) {
        toast(reply.message, "success", 2200);
        joinName.value = "";
        joinPass.value = "";
        joinFlag.value = "0";
        toggleJoin(false);
        selected.add(name);
        await loadSnapshot({ ping: true });
      } else {
        toast(`${reply.message}（code ${reply.code}）`, "error", 3200);
      }
    } catch (error) {
      toast(String(error), "error", 3200);
    } finally {
      submit.disabled = false;
      submit.textContent = "加入";
    }
  }

  refreshBtn.addEventListener("click", () => {
    pinged = false;
    pingMap.clear();
    void loadSnapshot({ ping: true });
  });
  searchEl.addEventListener("input", scheduleFilter);
  searchEl.addEventListener("compositionend", scheduleFilter);

  // 兜底：万一输入框在个别 WebView2 环境里拿不到键盘焦点，直接在页面上打字也能过滤
  document.addEventListener("keydown", (event) => {
    if (!container.isConnected) return;
    const target = event.target;
    const tag = target?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable) return;
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.key === "Backspace") {
      if (!searchEl.value) return;
      event.preventDefault();
      searchEl.value = searchEl.value.slice(0, -1);
      scheduleFilter();
      return;
    }
    if (event.key === "Escape") {
      if (!searchEl.value) return;
      searchEl.value = "";
      scheduleFilter();
      return;
    }
    if (event.key.length !== 1) return;
    event.preventDefault();
    searchEl.focus();
    searchEl.value += event.key;
    scheduleFilter();
  });
  onlineOnly.addEventListener("change", () => applyFilter());
  $("rm-ping").addEventListener("click", () => void runPing(true));
  joinToggle.addEventListener("click", () => toggleJoin(true));
  selAllBtn.addEventListener("click", () => {
    const total = snapshot?.networks.length || 0;
    if (total === 0) return;
    if (selected.size === total) selected.clear();
    else for (const network of snapshot.networks) selected.add(network.name);
    renderNets();
    onSelectionChanged();
  });
  $("rm-join-cancel").addEventListener("click", () => {
    toggleJoin(false);
    joinName.value = "";
    joinPass.value = "";
    joinFlag.value = "0";
  });
  $("rm-join-submit").addEventListener("click", () => void submitJoin());
  for (const input of [joinName, joinPass, joinFlag]) {
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") void submitJoin();
      if (event.key === "Escape") toggleJoin(false);
    });
  }

  await loadSnapshot();
  void runPing(true);

  const timer = setInterval(() => {
    if (disposed || !container.isConnected) {
      disposed = true;
      clearInterval(timer);
      if (filterTimer) window.clearTimeout(filterTimer);
      return;
    }
    if (joinOpen || pingPending) return;
    void loadSnapshot({ auto: true });
  }, REFRESH_MS);
}
