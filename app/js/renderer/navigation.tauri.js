// jshint esversion: 8

function getTauriWindow() {
  const w = window.__TAURI__?.window;
  if (!w) return null;
  if (typeof w.getCurrentWindow === 'function') return w.getCurrentWindow();
  if (w.appWindow) return w.appWindow;
  return null;
}

/*
  Prevent drop redirect
*/
$(document).on('drop', function(event) {
  event.preventDefault();
  return false;
});

$(document).on('dragover', function(event) {
  event.preventDefault();
  return false;
});

/*
  Minimize window button
*/
$(document).on('click', '#navButtonMinimize', async function() {
  const win = getTauriWindow();
  if (win?.minimize) await win.minimize();
});

/*
  Close main window button
*/
$(document).on('click', '#navButtonExit', async function() {
  const win = getTauriWindow();
  if (win?.close) await win.close();
});
