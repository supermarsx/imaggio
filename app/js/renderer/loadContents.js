// jshint esversion: 8

/*
  loadContents (self-executed)
    Loads application contents
*/
(async function loadContents() {
  loadHtml();
})();

/*
  Load language only when document is ready
*/
$(document).ready(() => {
  loadLanguage();
  return;
});

/*
  loadLanguage
    Loads the adequate language onto the app, defaults to english
*/
function loadLanguage() {
  var language = detectLanguage();

  var languageFile = getLanguageFile(language);

  for (var property in languageFile)
    if (languageFile.hasOwnProperty(property))
      $(`#${property}`).text(languageFile[property]);

  return;
}

/*
  getLanguageFile
    Gets the language file
*/
function getLanguageFile(language) {
  var languagePath = getLanguagePathObject();
  languagePath = { ...languagePath,
    language: language
  };
  var fullLanguagePath = getLanguageFullPathVar(languagePath);

  var jsonFile = null;
  $.ajax({
    'async': false,
    'global': false,
    'url': `${fullLanguagePath}`,
    'dataType': "json",
    'success': function(contents) {
      jsonFile = contents;
    }
  });

  return jsonFile;
}

function getLanguagePathObject() {
  return {
    path: 'js/locale/',
    extension: '.json'
  };
}

function getLanguageFullPathVar(languagePath) {
  return `${languagePath.path}${languagePath.language}${languagePath.extension}`;
}

/*
  loadHtml
    Loads HTML files inside the renderer
*/
function loadHtml() {
  var base = getHtmlPathObject();
  var fullPath = getHtmlFullPathObject(base);

  // Dynamically load through property, element id
  for (var property in fullPath)
    if (fullPath.hasOwnProperty(property))
      $(`#${property}`).load(fullPath[property]);

  return;
}

function getHtmlFullPathObject(baseVar) {
  var {
    subfolder,
    filenames
  } = baseVar;

  return {
    navTop: `${baseVar.path}${subfolder.nav}${filenames.navTop}`,
    p2Container: `${baseVar.path}${subfolder.tabs}${filenames.p2Container}`
  };
}

function getHtmlPathObject() {
  return {
    path: 'html/',
    subfolder: {
      nav: 'navigation/',
      tabs: 'tabs/'
    },
    filenames: {
      navTop: 'navTop.html',
      p2Container: 'p2.html'
    }
  };
}

function detectLanguage() {
  return navigator.language;
}
