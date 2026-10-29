import { confirmDialog, alertDialog, promptDialog } from "../ui.js";

const { invoke } = window.__TAURI__.core;

export async function render(container) {
  container.innerHTML = `
    <h2>运行日志</h2>
    <div class="log-box" id="log-box">（暂无日志）</div>
    <div class="row" style="margin-top:12px">
      <button id="btn-export">导出报告</button>
      <button id="btn-clear">清空日志</button>
      <label class="checkbox"><input type="checkbox" id="anon-check" /> 匿名提交</label>
    </div>
  `;

  const $ = (id) => document.getElementById(id);
  const logBox = $("log-box");

  async function refreshLogs() {
    try {
      const lines = await invoke("get_logs");
      if (!lines || lines.length === 0) {
        logBox.textContent = "（暂无日志）";
      } else {
        logBox.textContent = lines.join("\n");
        logBox.scrollTop = logBox.scrollHeight;
      }
    } catch (e) { logBox.textContent = "读取失败: " + e; }
  }

  $("btn-clear").addEventListener("click", async () => {
    const ok = await confirmDialog("确定清空所有日志？", "清空日志");
    if (!ok) return;
    await invoke("clear_logs");
    await refreshLogs();
  });

  $("btn-export").addEventListener("click", async () => {
    let defaultPath = "";
    try {
      const desktop = await invoke("get_desktop_path");
      defaultPath = desktop + "\\radmin_report_" + Date.now() + ".zip";
    } catch (e) { defaultPath = "radmin_report_" + Date.now() + ".zip"; }
    const path = await promptDialog("导出路径：", defaultPath, "导出报告");
    if (!path) return;
    try {
      await invoke("add_log", { msg: "导出报告到 " + path });
      await invoke("export_report", { path: path });
      await alertDialog("已导出到:\n" + path, "导出成功", { type: "success" });
      await refreshLogs();
    } catch (e) {
      await alertDialog(String(e), "导出失败", { type: "error" });
    }
  });

  try { const isAnon = await invoke("get_anonymous"); $("anon-check").checked = isAnon; } catch (e) {}

  $("anon-check").addEventListener("change", async () => {
    const checked = $("anon-check").checked;
    await invoke("set_anonymous", { anon: checked });
    await refreshLogs();
  });

  await refreshLogs();
  setInterval(refreshLogs, 2000);
}