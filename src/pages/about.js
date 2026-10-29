export function render(container) {
  const QQ_GROUP_NUMBER = "332824844";
  const QQ_GROUP_LINK = "https://qm.qq.com/q/6N7TJSaGGI";
  const DEV_QQ = "3942090401";
  const groupIconUrl = `https://p.qlogo.cn/gh/${QQ_GROUP_NUMBER}/${QQ_GROUP_NUMBER}/100/`;
  const devAvatarUrl = `https://q1.qlogo.cn/g?b=qq&nk=${DEV_QQ}&s=100`;

  container.innerHTML = `
    <div class="about-page">
      <div class="about-banner" id="banner-click">
        <img class="about-banner-img" src="${groupIconUrl}" onerror="this.style.display='none'" />
        <div class="about-banner-content">
          <h2 class="about-banner-title">MC综合交流群 radmin</h2>
          <p class="about-banner-desc">点击加入群聊</p>
          <div class="about-banner-info">
            <span>群号: <b>${QQ_GROUP_NUMBER}</b></span>
            <span>65 人</span>
          </div>
        </div>
      </div>
      <div class="about-dev">
        <h3>主开发</h3>
        <div class="about-dev-card">
          <img class="about-dev-avatar" src="${devAvatarUrl}" onerror="this.style.display='none'" />
          <div class="about-dev-info">
            <div class="about-dev-name">
              <span class="dev-qq dev-qq-copy" id="dev-qq" title="点击复制 QQ 号">QQ: <b>${DEV_QQ}</b></span>
            </div>
          </div>
        </div>
      </div>
      <div class="about-footer">
        <p>CrafterBox</p>
      </div>
    </div>
  `;

  document.getElementById("banner-click").addEventListener("click", async () => {
    try { await window.__TAURI__.core.invoke("open_url", { url: QQ_GROUP_LINK }); }
    catch (e) { try { await navigator.clipboard.writeText(QQ_GROUP_LINK); alert("已复制加群链接到剪贴板"); } catch {} }
  });

  function bindCopyQQ(elId, qq) {
    const el = document.getElementById(elId);
    if (!el) return;
    el.style.cursor = "pointer";
    el.addEventListener("click", async (e) => {
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(qq);
        const original = el.innerHTML;
        el.innerHTML = `<span style="color:#66ff88">已复制</span>`;
        setTimeout(() => { el.innerHTML = original; }, 1200);
      } catch { alert("QQ 号: " + qq); }
    });
  }
  bindCopyQQ("dev-qq", DEV_QQ);
}