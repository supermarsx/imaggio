// jshint esversion: 8

function getInvoke() {
  // Tauri v2: window.__TAURI__.core.invoke
  // Tauri v1: window.__TAURI__.invoke
  const coreInvoke = window.__TAURI__?.core?.invoke;
  const legacyInvoke = window.__TAURI__?.invoke;
  return coreInvoke || legacyInvoke;
}

async function invoke(command, args) {
  const inv = getInvoke();
  if (!inv) throw new Error('Tauri invoke API not available');
  return inv(command, args);
}

var options = {
  conversionType: null,
  inputFile: null
};

$(document).ready(() => {
  // Conversion type selection
  $(document).on('change', '#SOSelect', function() {
    var currentAction = this.value;
    options.conversionType = currentAction === 'null' ? null : currentAction;

    switch (currentAction) {
      case 'pdfmetaclone':
        showElementsToPdfClone();
        break;
      case 'pdf2join':
        showElementsToPdf2Join();
        break;
      default:
        showElementsForDefault();
        break;
    }
  });

  // Process folder tickbox
  $(document).on('click', '#SOProcessFolderTickbox', function() {
    if (options.conversionType === 'pdfmetaclone') return;

    if ($(this).is(':checked')) {
      $('#stepTwoLabel').addClass('is-hidden');
      $('#stepTwoLabel2').removeClass('is-hidden');
      $('#stepTwoButtonLabel').addClass('is-hidden');
      $('#stepTwoButtonLabel2').removeClass('is-hidden');
      $('#stepTwoDragLabel').addClass('is-hidden');
      $('#stepTwoDragLabel2').removeClass('is-hidden');
    } else {
      $('#stepTwoLabel').removeClass('is-hidden');
      $('#stepTwoLabel2').addClass('is-hidden');
      $('#stepTwoButtonLabel').removeClass('is-hidden');
      $('#stepTwoButtonLabel2').addClass('is-hidden');
      $('#stepTwoDragLabel').removeClass('is-hidden');
      $('#stepTwoDragLabel2').addClass('is-hidden');
    }
  });

  // Step 2: select file/folder (or clone-source file)
  $(document).on('click', '#S2OneButton', async function() {
    if (!options.conversionType) return;

    try {
      if (options.conversionType === 'pdfmetaclone') {
        // Step 2 for metadata clone = choose source
        const source = await invoke('select_file');
        if (!source) return;
        options.inputFile = source;
        return;
      }

      if ($('#SOProcessFolderTickbox').is(':checked')) {
        const folder = await invoke('select_folder');
        if (!folder) return;
        await convertFolder(folder);
      } else {
        const file = await invoke('select_file');
        if (!file) return;
        await convertOne(file);
      }
    } catch (err) {
      showFailure(err);
    }
  });

  // Step 3: select target file (pdfmetaclone)
  $(document).on('click', '#S3Button', async function() {
    if (options.conversionType !== 'pdfmetaclone') return;
    if (!options.inputFile) {
      showFailure(new Error('Select the source file first'));
      return;
    }

    try {
      const target = await invoke('select_file');
      if (!target) return;
      await convertOne(target);
    } catch (err) {
      showFailure(err);
    }
  });
});

async function convertOne(filePath, isMass = false) {
  showConverting(isMass);
  try {
    const res = await invoke('convert', {
      req: {
        inputPath: filePath,
        conversionType: options.conversionType,
        inputFile: options.inputFile
      }
    });

    // Reset clone source after a run
    if (options.conversionType === 'pdfmetaclone') options.inputFile = null;

    console.log('Converted:', res);
    if (!isMass) showSuccess();
    return res;
  } catch (err) {
    if (!isMass) showFailure(err);
    throw err;
  }
}

async function convertFolder(folderPath) {
  showConverting(true);
  const files = await invoke('list_dir', { dir_path: folderPath });
  const total = files.length;

  for (let i = 0; i < total; i++) {
    $('#MPLabel').text(`${i + 1}/${total}`);
    await convertOne(files[i], true);
  }

  showSuccess();
}

function showConverting(isMass) {
  $('#Start').addClass('is-hidden');
  if (isMass) {
    $('#MassProcessing').removeClass('is-hidden');
  } else {
    $('#Processing').removeClass('is-hidden');
  }
}

function showSuccess() {
  $('#Processing').addClass('is-hidden');
  $('#MassProcessing').addClass('is-hidden');
  $('#Start').removeClass('is-hidden');
  $('#ProcessingFinished').removeClass('is-hidden');

  setTimeout(() => {
    $('#ProcessingFinished').addClass('is-hidden');
  }, 2500);
}

function showFailure(err) {
  console.warn(err);
  $('#Processing').addClass('is-hidden');
  $('#MassProcessing').addClass('is-hidden');
  $('#Start').removeClass('is-hidden');
  $('#ProcessingFailed').removeClass('is-hidden');

  setTimeout(() => {
    $('#ProcessingFailed').addClass('is-hidden');
  }, 2500);
}

/*
  UI mode toggles (ported from Electron renderer)
*/
function showElementsToPdfClone() {
  $('#stepTwoLabel').removeClass('is-hidden');
  $('#stepTwoLabel2').addClass('is-hidden');
  $('#stepTwoButtonLabel').removeClass('is-hidden');
  $('#stepTwoButtonLabel2').addClass('is-hidden');
  $('#stepTwoDragLabel').removeClass('is-hidden');
  $('#stepTwoDragLabel2').addClass('is-hidden');

  $('#S2Two').css('display', 'none');
  $('#S2Three').css('display', 'none');

  $('#StepThree').removeClass('is-hidden');
  $('#SOProcessFolderContainer').css('display', 'none');
  return;
}

function showElementsToPdf2Join() {
  $('#S2Two').css('display', 'none');
  $('#S2Three').css('display', 'none');
  $('#SOProcessFolderTickbox').click();
}

function showElementsForDefault() {
  $('#S2Two').css('display', 'block');
  $('#S2Three').css('display', 'block');
  $('#StepThree').addClass('is-hidden');
  $('#SOProcessFolderContainer').css('display', 'block');

  if ($('#SOProcessFolderTickbox').is(':checked')) {
    $('#stepTwoLabel').addClass('is-hidden');
    $('#stepTwoLabel2').removeClass('is-hidden');
    $('#stepTwoButtonLabel').addClass('is-hidden');
    $('#stepTwoButtonLabel2').removeClass('is-hidden');
    $('#stepTwoDragLabel').addClass('is-hidden');
    $('#stepTwoDragLabel2').removeClass('is-hidden');
  }
}
