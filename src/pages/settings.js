const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

export async function render(container) {
  const cfg = window.__radminConfig.get();
  let currentAlpha = cfg.opacity;
  let currentTheme = [[46, 125, 91], [44, 95, 138], [94, 74, 138], [138, 106, 46], [138, 58, 58], [46, 122, 122]][cfg.theme_index];
  let currentEffect = cfg.effect;

  container.innerHTML = `
    <h2>设置</h2>
    <div class="row"><label>窗口效果:</label>
      <select id="effect-select" class="select-box">
        <option value="0">无</option><option value="2">透明渐变</option>
        <option value="4">亚克力</option><option value="5">云母</option>
      </select>
    </div>
    <div class="row"><label>透明度:</label><div class="slider-wrap"><input type="range" id="alpha-slider" min="0" max="100" value="${cfg.opacity}" /><span id="alpha-value">${cfg.opacity}</span></div></div>
    <div class="row"><label>窗口不透明度:</label><div class="slider-wrap"><input type="range" id="window-alpha-slider" min="0" max="100" value="${cfg.window_alpha}" /><span id="window-alpha-value">${cfg.window_alpha}</span></div></div>
    <div class="row"><label>主题色:</label><div id="theme-picker" style="display:flex;gap:10px"></div></div>
    <div class="row"><label class="checkbox"><input type="checkbox" id="top-check" ${cfg.top ? "checked" : ""} /> 窗口置顶</label></div>
    <div class="row"><label class="checkbox"><input type="checkbox" id="public-broadcast" ${cfg.public_broadcast ? "checked" : ""} /> 公益广播</label></div>
    <div class="row"><label class="checkbox"><input type="checkbox" id="thin-border" ${cfg.thin_border ? "checked" : ""} /> 显示细线框</label></div>
  `;

  const effectSelect = document.getElementById("effect-select");
  effectSelect.value = String(cfg.effect);
  const slider = document.getElementById("alpha-slider");
  const appEl = document.getElementById("app");

  async function applyAcrylic() {
    try { await invoke("apply_acrylic", { opacity: currentAlpha, r: currentTheme[0], g: currentTheme[1], b: currentTheme[2], effect: currentEffect }); } catch (e) {}
  }

  effectSelect.onchange = () => { currentEffect = parseInt(effectSelect.value); applyAcrylic(); window.__radminConfig.save({ effect: currentEffect }); };
  slider.oninput = () => { currentAlpha = parseInt(slider.value); document.getElementById("alpha-value").textContent = currentAlpha; applyAcrylic(); window.__radminConfig.save({ opacity: currentAlpha }); };

  const THEMES = [[46, 125, 91], [44, 95, 138], [94, 74, 138], [138, 106, 46], [138, 58, 58], [46, 122, 122]];
  const picker = document.getElementById("theme-picker");
  THEMES.forEach((rgb, idx) => {
    const btn = document.createElement("div");
    btn.className = "swatch";
    btn.style.background = `rgb(${rgb.join(",")})`;
    btn.style.width = btn.style.height = "36px";
    if (idx === cfg.theme_index) btn.style.borderColor = "#fff";
    btn.onclick = () => {
      picker.querySelectorAll(".swatch").forEach(b => b.style.borderColor = "rgba(255,255,255,0.4)");
      btn.style.borderColor = "#fff";
      currentTheme = rgb;
      applyAcrylic();
      window.__radminConfig.save({ theme_index: idx });
    };
    picker.appendChild(btn);
  });

  document.getElementById("top-check").onchange = async (e) => {
    try { await getCurrentWindow().setAlwaysOnTop(e.target.checked); } catch (err) {}
    window.__radminConfig.save({ top: e.target.checked });
  };
  
  document.getElementById("public-broadcast").onchange = async (e) => {
    try { await invoke("set_public_broadcast", { enabled: e.target.checked }); } catch (err) {}
    window.__radminConfig.save({ public_broadcast: e.target.checked });
  };
  
  document.getElementById("thin-border").onchange = (e) => {
    if (e.target.checked) document.body.classList.remove("no-thin-border");
    else document.body.classList.add("no-thin-border");
    window.__radminConfig.save({ thin_border: e.target.checked });
  };

  const wSlider = document.getElementById("window-alpha-slider");
  const wValue = document.getElementById("window-alpha-value");
  wSlider.oninput = () => {
    wValue.textContent = wSlider.value;
    appEl.style.background = `rgba(16, 16, 24, ${wSlider.value / 100})`;
    window.__radminConfig.save({ window_alpha: parseInt(wSlider.value) });
  };

  applyAcrylic();
}