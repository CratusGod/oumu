// "Translate with Oumu" on the right-click menu of highlighted text. The text
// goes to Oumu through native messaging (host com.cratusgod.oumu, which is
// the oumu binary); Oumu slides its panel out with the translation.
const api = globalThis.browser ?? globalThis.chrome;
const MENU_ID = "oumu-translate";
const HOST = "com.cratusgod.oumu";

// Menus made at install persist across browser restarts.
api.runtime.onInstalled.addListener(() => {
  api.contextMenus.create({ id: MENU_ID, title: "Translate with Oumu", contexts: ["selection"] });
});

api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== MENU_ID || !info.selectionText) return;
  try {
    const reply = await api.runtime.sendNativeMessage(HOST, { action: "translate", text: info.selectionText });
    if (!reply || !reply.ok) console.warn("Oumu could not translate:", reply && reply.error);
  } catch (e) {
    console.warn("Oumu is not installed, or its browser bridge is missing:", e);
  }
});
