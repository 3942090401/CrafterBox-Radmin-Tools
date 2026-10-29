const Tauri = window.__TAURI__;
if (!Tauri) throw new Error("__TAURI__ unavailable");
const invoke = Tauri.core.invoke;
const appWindow = Tauri.window.getCurrentWindow();

const CATEGORIES = {
  info:    { hasSub: false, page: "info" },
  account: { hasSub: false, page: "account" },
  lan:     { hasSub: true,
             subs: [
               { id: "lan-radmin", name: "Radmin" },
               { id: "broadcast",   name: "广播" },
               { id: "scan",        name: "扫描" },
             ] },
  options: { hasSub: true,
             subs: [
               { id: "settings", name: "设置" },
               { id: "log",      name: "日志" },
             ] },
  about:   { hasSub: false, page: "about" },
};

const lastSubOf = { lan: "lan-radmin", options: "settings" };
const tabs = document.querySelectorAll(".tab");
const subNav = document.getElementById("sub-nav");
const contentArea = document.getElementById("content-area");

let currentCategory = "info";
let currentPageEl = null;

async function loadPage(name) {
  if (currentPageEl) {
    contentArea.classList.remove("slide-in");
    contentArea.classList.add("slide-out");
    await new Promise(r => setTimeout(r, 200));
  }
  contentArea.innerHTML = "";
  contentArea.scrollTop = 0;
  contentArea.classList.remove("slide-out");

  try {
    const mod = await import(`./pages/${name}.js`);
    const div = document.createElement("div");
    div.className = "page-content";
    contentArea.appendChild(div);
    mod.render(div);
    currentPageEl = div;
    void contentArea.offsetWidth;
    contentArea.classList.add("slide-in");
    setTimeout(() => enhanceNumberInputs(div), 50);
  } catch (e) {
    contentArea.innerHTML = `<div style="color:#f66;padding:20px">加载失败: ${e.message}</div>`;
    console.error(`loadPage ${name}:`, e);
  }
}

function renderSubNav(category) {
  const cat = CATEGORIES[category];
  if (!cat.hasSub) {
    subNav.style.display = "none";
    subNav.innerHTML = "";
    return;
  }
  subNav.style.display = "flex";
  subNav.innerHTML = "";
  cat.subs.forEach((sub, index) => {
    const btn = document.createElement("button");
    btn.className = "sub-item";
    btn.dataset.sub = sub.id;
    btn.textContent = sub.name;
    if (sub.id === lastSubOf[category]) btn.classList.add("active");
    btn.style.animationDelay = `${0.02 + index * 0.05}s`;
    btn.addEventListener("click", () => {
      subNav.querySelectorAll(".sub-item").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
      lastSubOf[category] = sub.id;
      loadPage(sub.id);
    });
    subNav.appendChild(btn);
  });
}

async function switchCategory(category) {
  if (category === currentCategory && currentPageEl) return;
  tabs.forEach(t => t.classList.toggle("active", t.dataset.tab === category));
  currentCategory = category;
  const cat = CATEGORIES[category];
  if (cat.hasSub) {
    const subId = lastSubOf[category] || cat.subs[0].id;
    lastSubOf[category] = subId;
    renderSubNav(category);
    await loadPage(subId);
  } else {
    subNav.style.display = "none";
    subNav.innerHTML = "";
    await loadPage(cat.page);
  }
}

tabs.forEach(tab => tab.addEventListener("click", () => switchCategory(tab.dataset.tab)));

let globalConfig = null;

const DEFAULT_CFG = {
  effect: 4, opacity: 30, window_alpha: 25, theme_index: 0,
  top: false, thin_border: true, public_broadcast: true,
  accounts: [],
  broadcast_motd: "服务器{num}", broadcast_count: "10", broadcast_prefix: "", port_pad: false,
  scan_max_per_ip: 3,
  tcp_target: "127.0.0.1:25565", tcp_size: 1024, tcp_threads: 10, tcp_rate: 0, tcp_duration: 0,
  motd_target: "127.0.0.1:25565", motd_threads: 10, motd_rate: 0, motd_duration: 0
};

async function initApp() {
  try {
    globalConfig = await invoke("get_app_config");
  } catch (e) {
    console.warn("config load failed, using default", e);
    globalConfig = Object.assign({}, DEFAULT_CFG);
  }

  // 补全可能缺失的字段
  for (let k in DEFAULT_CFG) {
    if (globalConfig[k] === undefined) globalConfig[k] = DEFAULT_CFG[k];
  }

  const THEMES = [
    [46, 125, 91], [44, 95, 138], [94, 74, 138],
    [138, 106, 46], [138, 58, 58], [46, 122, 122]
  ];
  const theme = THEMES[globalConfig.theme_index] || THEMES[0];

  // 启动时应用毛玻璃
  try {
    await invoke("apply_acrylic", {
      opacity: globalConfig.opacity,
      r: theme[0], g: theme[1], b: theme[2],
      effect: globalConfig.effect
    });
  } catch (e) {}

  // 启动时应用窗口透明度
  const appEl = document.getElementById("app");
  if (appEl) appEl.style.background = `rgba(16, 16, 24, ${globalConfig.window_alpha / 100})`;

  if (!globalConfig.thin_border) document.body.classList.add("no-thin-border");
  if (globalConfig.top) { try { await appWindow.setAlwaysOnTop(true); } catch (e) {} }
  try { await invoke("set_public_broadcast", { enabled: globalConfig.public_broadcast }); } catch (e) {}

  // 绑定窗口按钮
  document.getElementById("btn-min").addEventListener("click", async (e) => {
    e.preventDefault(); e.stopPropagation();
    try { await appWindow.minimize(); } catch (err) {}
  });
  document.getElementById("btn-close").addEventListener("click", async (e) => {
    e.preventDefault(); e.stopPropagation();
    try { await appWindow.close(); } catch (err) {}
  });

  switchCategory("info");
}

function enhanceNumberInputs(root) {
  const nums = (root || document).querySelectorAll('input[type="number"]:not([data-enhanced])');
  nums.forEach(input => {
    input.dataset.enhanced = "1";
    const wrap = document.createElement("div");
    wrap.className = "num-wrap";
    input.parentNode.insertBefore(wrap, input);
    wrap.appendChild(input);
    const btns = document.createElement("div");
    btns.className = "num-btns";
    btns.innerHTML = '<button class="num-btn" data-dir="up" tabindex="-1">+</button><button class="num-btn" data-dir="down" tabindex="-1">−</button>';
    wrap.appendChild(btns);
    const step = (dir) => {
      const min = input.min !== "" ? parseFloat(input.min) : -Infinity;
      const max = input.max !== "" ? parseFloat(input.max) : Infinity;
      let v = parseFloat(input.value) || 0;
      v = Math.max(min, Math.min(max, v + (dir === "up" ? 1 : -1)));
      input.value = v;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    };
    btns.querySelector('[data-dir="up"]').onclick = (e) => { e.preventDefault(); step("up"); };
    btns.querySelector('[data-dir="down"]').onclick = (e) => { e.preventDefault(); step("down"); };
  });
}

window.__radminConfig = {
  get: () => globalConfig,
  save: async (partial) => {
    globalConfig = Object.assign({}, globalConfig, partial);
    try { await invoke("save_app_config", { cfg: globalConfig }); } catch (e) {}
  }
};

initApp();