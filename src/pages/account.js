const { invoke } = window.__TAURI__.core;
import { confirmDialog, promptDialog, toast } from "../ui.js";

export async function render(container) {
  container.innerHTML = `
    <div class="account-header account-toolbar">
      <h2>账号管理</h2>
      <span class="account-toolbar-spacer"></span>
      <button id="btn-add-account">保存当前账号</button>
      <button id="btn-save-profile">保存配置</button>
      <button id="btn-open-profiles">配置文件</button>
      <button id="btn-open-folder">打开配置文件夹</button>
    </div>
    <input id="profile-file-input" type="file" accept=".json,application/json" hidden />
    <div class="account-list" id="account-list"></div>
  `;

  const $ = id => document.getElementById(id);
  const listEl = $("account-list");
  let accounts = [];
  let profiles = [];
  let selectedProfile = null;
  let profileOverlay = null;
  let profileKeyHandler = null;

  function closeProfileManager() {
    const overlay = profileOverlay;
    profileOverlay = null;
    document.removeEventListener("click", onProfileOutsideClick, true);
    if (profileKeyHandler) {
      document.removeEventListener("keydown", profileKeyHandler);
      profileKeyHandler = null;
    }
    document.querySelectorAll(".profile-modal-overlay").forEach(element => element.remove());
    if (overlay) overlay.remove();
  }

  function onProfileOutsideClick(event) {
    if (!profileOverlay || !(event.target instanceof Element)) return;
    if (event.target.closest(".profile-modal-panel, #__modal_root")) return;
    closeProfileManager();
  }

  function escapeHtml(value) {
    return String(value).replace(/[&<>"']/g, character => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
    })[character]);
  }

  async function refreshAccounts() {
    accounts = await invoke("get_accounts");
    renderAccounts();
  }

  function renderAccounts() {
    if (accounts.length === 0) {
      listEl.innerHTML = '<div style="color:#888;text-align:center;padding:40px">暂无账号，点击上方保存当前账号</div>';
      return;
    }
    listEl.innerHTML = accounts.map((account, index) => `
      <div class="account-card" title="${escapeHtml(`名字：${account.alias}\nIPv4：${account.ipv4}\nIPv6：${account.ipv6 || ""}\nRID：${account.rid}`)}">
        <div class="acc-info">
          <div class="acc-name">${escapeHtml(account.alias)}</div>
          <div class="acc-meta">RID: ${escapeHtml(account.rid)} | IP: ${escapeHtml(account.ipv4)}</div>
        </div>
        <div class="acc-actions">
          <button data-action="apply-account" data-index="${index}" ${account.ServerPassword?.length ? "" : "disabled"}>切换</button>
          <button data-action="rename-account" data-index="${index}">重命名</button>
          <button data-action="up-account" data-index="${index}" ${index === 0 ? "disabled" : ""}>上移</button>
          <button data-action="down-account" data-index="${index}" ${index === accounts.length - 1 ? "disabled" : ""}>下移</button>
          <button class="btn-del" data-action="delete-account" data-index="${index}">删除</button>
        </div>
      </div>
    `).join("");
  }

  async function saveAccounts() {
    await invoke("save_accounts", { accounts });
    renderAccounts();
  }

  listEl.addEventListener("click", async event => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const index = Number(button.dataset.index);
    const account = accounts[index];
    if (!account) return;
    try {
      switch (button.dataset.action) {
        case "apply-account":
          await invoke("apply_account", { account });
          toast(`已切换到 ${account.alias}`);
          break;
        case "rename-account": {
          const name = await promptDialog("请输入新的账号名称：", account.alias, "重命名");
          if (name?.trim()) {
            account.alias = name.trim();
            await saveAccounts();
            toast("账号名称已更新");
          }
          break;
        }
        case "up-account":
          if (index > 0) {
            [accounts[index - 1], accounts[index]] = [accounts[index], accounts[index - 1]];
            await saveAccounts();
          }
          break;
        case "down-account":
          if (index < accounts.length - 1) {
            [accounts[index + 1], accounts[index]] = [accounts[index], accounts[index + 1]];
            await saveAccounts();
          }
          break;
        case "delete-account":
          if (await confirmDialog(`确定删除账号 ${account.alias}？`, "删除")) {
            accounts.splice(index, 1);
            await saveAccounts();
            toast("账号已删除");
          }
          break;
      }
    } catch (error) {
      toast(String(error), "error", 3600);
    }
  });

  $("btn-add-account").onclick = async () => {
    try {
      const info = await invoke("get_radmin_info");
      const name = await promptDialog("请输入账号名称：", info.alias || `账号_${info.rid}`, "保存当前账号");
      if (!name?.trim()) return;
      const ipv6 = await invoke("get_current_ipv6");
      accounts.push({
        alias: name.trim(),
        ipv4: info.ip,
        ipv6,
        rid: info.rid,
        ServerPassword: info.password
      });
      await saveAccounts();
      toast("当前账号已保存");
    } catch (error) {
      toast(`保存账号失败：${error}`, "error", 3600);
    }
  };

  async function refreshProfiles() {
    profiles = await invoke("list_account_profiles");
    if (!profiles.some(profile => profile.file === selectedProfile)) {
      selectedProfile = profiles[0]?.file || null;
    }
    renderProfiles();
  }

  function renderProfiles() {
    if (!profileOverlay) return;
    const rows = profileOverlay.querySelector(".profile-rows");
    if (!profiles.length) {
      rows.innerHTML = '<div class="profile-empty">暂无配置文件。点击下方“导出”保存当前账号列表，或导入 JSON 文件。</div>';
      return;
    }
    rows.innerHTML = profiles.map((profile, index) => `
      <div class="profile-row ${profile.file === selectedProfile ? "selected" : ""}" data-file="${escapeHtml(profile.file)}">
        <div class="profile-row-name" title="${escapeHtml(profile.file)}">${escapeHtml(profile.name)}</div>
        <div class="profile-row-actions">
          <button data-action="activate-profile" data-file="${escapeHtml(profile.file)}">切换</button>
          <button class="btn-del" data-action="delete-profile" data-file="${escapeHtml(profile.file)}">删除</button>
          <button data-action="rename-profile" data-file="${escapeHtml(profile.file)}">重命名</button>
          <button data-action="up-profile" data-file="${escapeHtml(profile.file)}" ${index === 0 ? "disabled" : ""}>上移</button>
          <button data-action="down-profile" data-file="${escapeHtml(profile.file)}" ${index === profiles.length - 1 ? "disabled" : ""}>下移</button>
        </div>
      </div>
    `).join("");
  }

  async function openProfileManager() {
    if (profileOverlay) return;
    document.querySelectorAll(".profile-modal-overlay").forEach(element => element.remove());
    await refreshProfiles();
    profileOverlay = document.createElement("div");
    profileOverlay.className = "profile-modal-overlay";
    profileOverlay.setAttribute("role", "dialog");
    profileOverlay.setAttribute("aria-modal", "true");
    profileOverlay.innerHTML = `
      <section class="profile-modal-panel">
        <div class="profile-modal-head">
          <div class="modal-title">配置文件</div>
          <button type="button" class="modal-btn profile-modal-close" aria-label="关闭">×</button>
        </div>
        <div class="modal-body"><div class="profile-rows"></div></div>
        <div class="profile-modal-actions">
          <button type="button" class="modal-btn" data-action="export-profile">导出</button>
          <button type="button" class="modal-btn modal-ok" data-action="import-profile">导入</button>
        </div>
      </section>
    `;
    const overlay = profileOverlay;
    overlay.querySelector(".profile-modal-close").addEventListener("click", event => {
        event.preventDefault();
        event.stopPropagation();
        closeProfileManager();
    });
    overlay.querySelector(".profile-modal-panel").addEventListener("click", async event => {
        const button = event.target.closest("button[data-action]");
        if (!button) {
          const row = event.target.closest(".profile-row");
          if (row) {
            selectedProfile = row.dataset.file;
            renderProfiles();
          }
          return;
        }
        const action = button.dataset.action;
        const file = button.dataset.file;
        try {
          if (action === "activate-profile") {
            accounts = await invoke("activate_account_profile", { name: file });
            selectedProfile = file;
            renderAccounts();
            toast(`已切换配置：${profiles.find(profile => profile.file === file)?.name || file}`);
          } else if (action === "delete-profile") {
            const profile = profiles.find(item => item.file === file);
            if (await confirmDialog(`确定删除配置文件 ${profile?.name || file}？`, "删除配置")) {
              await invoke("delete_account_profile", { name: file });
              await refreshProfiles();
              toast("配置文件已删除");
            }
          } else if (action === "rename-profile") {
            const profile = profiles.find(item => item.file === file);
            const name = await promptDialog("输入新的配置文件名称：", profile?.name || file, "重命名配置");
            if (name?.trim()) {
              selectedProfile = await invoke("rename_account_profile", { file, name: name.trim() });
              await refreshProfiles();
              toast("配置文件已重命名");
            }
          } else if (action === "up-profile" || action === "down-profile") {
            await invoke("move_account_profile", { file, direction: action === "up-profile" ? -1 : 1 });
            selectedProfile = file;
            await refreshProfiles();
          } else if (action === "export-profile") {
            if (!selectedProfile) {
              toast("请先选择一个配置文件", "info");
              return;
            }
            const profile = profiles.find(item => item.file === selectedProfile);
            const contents = await invoke("read_account_profile", { name: selectedProfile });
            downloadJson(contents, profile?.file || selectedProfile);
            toast("配置文件已导出");
          } else if (action === "import-profile") {
            $("profile-file-input").click();
          }
        } catch (error) {
          toast(String(error), "error", 3600);
        }
      });
    profileKeyHandler = event => {
      if (event.key === "Escape" && profileOverlay === overlay) {
        closeProfileManager();
      }
    };
    document.addEventListener("keydown", profileKeyHandler);
    document.addEventListener("click", onProfileOutsideClick, true);
    document.body.appendChild(overlay);
    renderProfiles();
    overlay.querySelector(".profile-modal-close").focus();
  }

  function downloadJson(contents, filename) {
    const url = URL.createObjectURL(new Blob([contents], { type: "application/json;charset=utf-8" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = filename.endsWith(".json") ? filename : `${filename}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  $("btn-save-profile").onclick = async () => {
    try {
      const file = await invoke("save_account_profile", { accounts });
      await refreshProfiles();
      selectedProfile = file;
      renderProfiles();
      toast(`配置已保存：${file}`);
    } catch (error) {
      toast(`保存配置失败：${error}`, "error", 3600);
    }
  };

  $("btn-open-profiles").onclick = openProfileManager;
  $("btn-open-folder").onclick = async () => {
    try {
      await invoke("open_account_config_folder");
      toast("已打开配置文件夹");
    } catch (error) {
      toast(String(error), "error", 3600);
    }
  };

  $("profile-file-input").onchange = async event => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    try {
      selectedProfile = await invoke("import_account_profile", { contents: await file.text() });
      await refreshProfiles();
      if (profileOverlay) renderProfiles();
      toast(`配置已导入：${selectedProfile}`);
    } catch (error) {
      toast(`导入配置失败：${error}`, "error", 3600);
    }
  };

  try {
    await refreshAccounts();
  } catch (error) {
    toast(`读取账号失败：${error}`, "error", 3600);
  }
}
