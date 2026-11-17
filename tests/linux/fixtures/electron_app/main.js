const { app, BrowserWindow } = require("electron");
app.whenReady().then(() => {
  const w = new BrowserWindow({ width: 700, height: 400, title: "oumu-electron-test" });
  w.loadFile("index.html");
  w.on("page-title-updated", (e) => e.preventDefault());
});
