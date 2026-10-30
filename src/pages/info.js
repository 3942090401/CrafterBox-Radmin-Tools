import { alertDialog, toast } from "../ui.js";

const { invoke } = window.__TAURI__.core;

export async function render(container) {
  container.innerHTML = `
    <h2>设备信息</h2>
    <div class="row"><label>IP 地址:</label><div class="value-box" id="ip-box">加载中...</div><button id="btn-random-ip">随机</button></div>
    <div class="row"><label>MAC 地址:</label><div class="value-box" id="mac-box">加载中...</div><button id="btn-random-mac">随机</button></div>
    <div class="row"><label>设备名:</label><input id="alias-input" placeholder="设备名" /><button id="btn-random-alias">随机</button><label>RID:</label><input id="rid-input" placeholder="RID" style="flex:0.5" /><button id="btn-apply-rid">应用</button></div>
    <div class="row"><label>加入网络:</label><button id="btn-goto-join">打开加网页面 →</button></div>
    <h2 style="margin-top:20px">网卡详细信息</h2>
    <div class="detail-box" id="detail-box">加载中...</div>
  `;

  const $ = (id) => document.getElementById(id);

  async function refresh() {
    if (!$("ip-box")) return;
    try {
      const info = await invoke("get_radmin_info");
      if (!$("ip-box")) return;
      $("ip-box").textContent = info.ip || "-";
      $("mac-box").textContent = info.mac || "-";
      $("alias-input").value = info.alias || "";
      $("rid-input").value = info.rid || "";
    } catch (e) { console.error(e); }
    try { const detail = await invoke("get_radmin_detail"); if ($("detail-box")) $("detail-box").textContent = detail || "(无数据)"; } catch (e) {}
  }

  $("btn-random-ip").onclick = async () => {
    try { const r = await invoke("random_ip"); toast(r); setTimeout(refresh, 2000); }
    catch (e) { toast(String(e), "error", 3600); }
  };

  $("btn-random-mac").onclick = async () => {
    try { const r = await invoke("random_mac"); toast("MAC: " + r); setTimeout(refresh, 2000); }
    catch (e) { toast(String(e), "error", 3600); }
  };

  $("btn-random-alias").onclick = async () => {
    try { $("alias-input").value = await invoke("random_alias"); } catch (e) { alert(e); }
  };

  $("btn-apply-rid").onclick = async () => {
    const rid = $("rid-input").value.trim(); const alias = $("alias-input").value.trim();
    if (!rid && !alias) return toast("请输入 RID 或设备名", "info");
    try { const r = await invoke("apply_rid_alias", { rid, alias }); toast(r); setTimeout(refresh, 2000); }
    catch (e) { toast(String(e), "error", 3600); }
  };

  $("btn-goto-join").onclick = () => {
    // 跳到 LAN → Radmin，并让该页自动展开加网表单
    window.__rmOpenJoin = true;
    const tab = document.querySelector('.tab[data-tab="lan"]');
    if (!tab) return;
    tab.click();
    window.setTimeout(() => {
      if (!document.getElementById("rm-join-form")) {
        document.querySelector('.sub-item[data-sub="lan-radmin"]')?.click();
      }
    }, 420);
  };

  await refresh();
}