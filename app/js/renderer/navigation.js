// jshint esversion: 8

function getTauriWindow() {
  const w = window.__TAURI__?.window;
  if (!w) return null;
  if (typeof w.getCurrentWindow === 'function') return w.getCurrentWindow();
  if (w.appWindow) return w.appWindow;
  return null;
}

/*
  Theme: restore saved preference or detect system preference
*/
(function initTheme() {
  var saved = localStorage.getItem('theme');
  var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  var theme = saved || (prefersDark ? 'dark' : 'light');
  document.documentElement.setAttribute('data-theme', theme);
})();

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
  Theme toggle button
*/
$(document).on('click', '#navButtonTheme', function() {
  var current = document.documentElement.getAttribute('data-theme');
  var next = current === 'dark' ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', next);
  localStorage.setItem('theme', next);
  $(this).find('i').toggleClass('fa-moon fa-sun');
});

/*
  Update theme icon when navbar loads
*/
$(document).on('DOMNodeInserted', '#navTop', function() {
  var theme = document.documentElement.getAttribute('data-theme');
  if (theme === 'dark') {
    $('#navButtonTheme i').removeClass('fa-moon').addClass('fa-sun');
  }
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
