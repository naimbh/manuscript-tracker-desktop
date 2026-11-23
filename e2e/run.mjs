// End-to-end tour of the desktop app through tauri-driver (Linux: WebKitWebDriver, Windows: msedgedriver).
// It starts a debug build with an empty data folder, walks through the main screens with fictional demo data
// and saves a screenshot of every step in e2e/screenshots.
//
// Environment:
//   APP            path to the debug executable (default: ../src-tauri/target/debug/manuscript-tracker[.exe])
//   TAURI_DRIVER   path to tauri-driver (default: tauri-driver on PATH)
//   NATIVE_DRIVER  path to the platform WebDriver (Windows: msedgedriver.exe)

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';
import { Builder, By, Key, until } from 'selenium-webdriver';

const here = path.dirname(fileURLToPath(import.meta.url));
const exe = process.platform === 'win32' ? 'manuscript-tracker.exe' : 'manuscript-tracker';
const app = path.resolve(process.env.APP || path.join(here, '..', 'src-tauri', 'target', 'debug', exe));
const shots = path.join(here, 'screenshots');
const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mt-e2e-'));

fs.rmSync(shots, { recursive: true, force: true });
fs.mkdirSync(shots, { recursive: true });

if (!fs.existsSync(app)) {
  console.error(`App not found: ${app}`);
  process.exit(1);
}

// A one-page fictional PDF to attach and preview.
const samplePdf = path.join(dataDir, 'sample-manuscript.pdf');
fs.writeFileSync(samplePdf, makePdf('Field Notes on Offline-First Clinic Records (sample manuscript)'));

// The app reads these at start-up (debug builds only): a separate data folder, no file dialogs, and the file to “pick”.
// A generated portrait-sized image for the profile photo cropper.
const samplePhoto = path.join(dataDir, 'sample-photo.png');
fs.writeFileSync(samplePhoto, makePng(320, 240));

const env = { ...process.env, MT_DATA_DIR: dataDir, MT_E2E: '1', MT_E2E_PICK: samplePdf, MT_QA_PICK: samplePhoto };
// Windows: talk to msedgedriver directly (what tauri-driver would do), with its log kept beside the screenshots.
const edge = process.platform === 'win32' && process.env.NATIVE_DRIVER;
const tauriDriver = edge
  ? spawn(process.env.NATIVE_DRIVER, ['--port=4444', '--verbose', `--log-path=${path.join(shots, 'msedgedriver.log')}`], { env, stdio: ['ignore', 'inherit', 'inherit'] })
  : spawn(process.env.TAURI_DRIVER || 'tauri-driver', [], { env, stdio: ['ignore', 'inherit', 'inherit'] });
const capabilities = edge
  ? { browserName: 'webview2', 'ms:edgeOptions': { binary: app, args: [], webviewOptions: {} } }
  : { browserName: 'wry', 'tauri:options': { application: app } };

let driver;
let step = 0;
const failures = [];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function shot(name) {
  step += 1;
  const file = path.join(shots, `${String(step).padStart(2, '0')}-${name}.png`);
  await sleep(600);
  fs.writeFileSync(file, await driver.takeScreenshot(), 'base64');
  console.log(`  saved ${path.basename(file)}`);
}

async function check(name, fn) {
  console.log(`- ${name}`);
  try {
    await fn();
  } catch (e) {
    failures.push(`${name}: ${e.message.split('\n')[0]}`);
    console.error(`  FAILED: ${e.message.split('\n')[0]}`);
    try {
      await shot(`${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-failed`);
    } catch {
      // the window may be gone
    }
  }
}

const xpathText = (tag, text) => By.xpath(`//${tag}[contains(normalize-space(.), ${JSON.stringify(text)})]`);

async function visible(locator, timeout = 15000) {
  const el = await driver.wait(until.elementLocated(locator), timeout);
  await driver.wait(until.elementIsVisible(el), timeout);
  return el;
}

async function clickText(text, tag = 'button') {
  const els = await driver.findElements(xpathText(tag, text));
  for (const el of els) {
    if (await el.isDisplayed()) {
      await el.click();
      return;
    }
  }
  throw new Error(`No visible ${tag} with text “${text}”`);
}

async function type(name, value, scope) {
  const el = await visible(By.css(`${scope ? scope + ' ' : ''}[name="${name}"]`));
  await el.clear();
  await el.sendKeys(value);
}

async function go(hash) {
  await driver.executeScript(`location.hash = ${JSON.stringify(hash)}`);
  await sleep(900);
}

async function pageHas(text, timeout = 15000) {
  await driver.wait(
    async () => driver.executeScript('return document.body.innerText.includes(arguments[0])', text),
    timeout,
    `Page never showed “${text}”`,
  );
}

// The innermost visible element whose text is exactly `text`.
async function byExactText(text, timeout = 15000) {
  let el = null;
  await driver.wait(async () => {
    el = await driver.executeScript(
      `const t = arguments[0];
       const all = [...document.querySelectorAll('body *')].filter((e) => e.offsetParent !== null && e.textContent.trim() === t);
       return all.find((e) => ![...e.children].some((c) => c.textContent.trim() === t)) || null;`,
      text,
    );
    return el !== null;
  }, timeout, `Nothing shows “${text}”`);
  return el;
}

// Popovers live in the top layer, which some WebDrivers do not report as visible.
async function popoverOpen(selector = '[data-flux-date-popover]', timeout = 10000) {
  await driver.wait(
    () => driver.executeScript(`return [...document.querySelectorAll(arguments[0])].some((p) => p.getBoundingClientRect().height > 0);`, selector),
    timeout,
    'The popover did not open',
  );
  await sleep(300);
}

async function api(name, args = {}) {
  return driver.executeAsyncScript(
    `const done = arguments[arguments.length - 1];
     window.__TAURI_INTERNALS__.invoke('api', { name: ${JSON.stringify(name)}, args: ${JSON.stringify(args)} })
       .then((r) => done({ ok: r }), (e) => done({ error: e }));`,
  );
}

async function setTheme(theme) {
  await api('app.setTheme', { theme });
  await driver.navigate().refresh();
  await sleep(1500);
}

async function tour() {
  await sleep(1500);
  driver = await new Builder().usingServer('http://127.0.0.1:4444/').withCapabilities(capabilities).build();
  await driver.manage().setTimeouts({ script: 30000 });

  // Collect script errors from the page (re-armed after every reload).
  const watchErrors = () =>
    driver.executeScript(`if (!window.__e2eErrors) { window.__e2eErrors = [];
      addEventListener('error', (e) => window.__e2eErrors.push(String(e.message)));
      addEventListener('unhandledrejection', (e) => window.__e2eErrors.push(String(e.reason && (e.reason.message || e.reason))));
      const oe = console.error; console.error = (...a) => { window.__e2eErrors.push(a.map(String).join(' ')); oe(...a); }; }`);
  const pageErrors = [];
  const collectErrors = async () => {
    const errs = await driver.executeScript('return window.__e2eErrors || []').catch(() => []);
    pageErrors.push(...errs);
    await driver.executeScript('window.__e2eErrors = []').catch(() => {});
  };
  const escape = async () => {
    await driver.actions().sendKeys(Key.ESCAPE).perform();
    await sleep(400);
  };
  let paperId = null;

  await check('Onboarding', async () => {
    await visible(By.css('input[name="name"]'), 30000);
    await watchErrors();
    await shot('light-welcome');
    await type('name', 'Alex Demo');
    await type('email', 'alex@example.com');
    await clickText('Continue');
    await visible(By.css('textarea[name="interests"]'));
    await type('interests', 'human-computer interaction, digital health, platform work');
    await shot('light-welcome-research');
    await clickText('Open my desk');
    await pageHas('Good ', 20000);
    await watchErrors();
    await shot('light-desk-empty');
  });

  await check('New paper', async () => {
    await clickText('New paper');
    await visible(By.css('dialog[data-modal="new-paper"] input[name="title"]'));
    await type('title', 'Field Notes on Offline-First Clinic Records', 'dialog[data-modal="new-paper"]');
    await type('method', '24 interviews and a design workshop', 'dialog[data-modal="new-paper"]');
    await shot('light-new-paper');
    await clickText('Add paper');
    await pageHas('Field Notes on Offline-First Clinic Records');
    const url = await driver.getCurrentUrl();
    paperId = (url.match(/#\/papers\/(\d+)/) || [])[1] || null;
    await shot('light-paper-opened');
  });

  await check('Open the paper from the list', async () => {
    await go('#/papers');
    await pageHas('Field Notes on Offline-First Clinic Records');
    await (await byExactText('Field Notes on Offline-First Clinic Records')).click();
    await visible(xpathText('button', 'Record submission'));
    await shot('light-paper-detail');
  });

  await check('Add a submission with the date picker', async () => {
    await clickText('Record submission');
    const scope = 'dialog[data-modal="sub-form"]';
    await visible(By.css(`${scope} input[name="venue_name"]`));
    await type('venue_name', 'Example Journal of Field Studies', scope);
    await type('manuscript_id', 'EJFS-2026-0042', scope);
    await (await visible(By.css(`${scope} button[aria-label="Open calendar"]`))).click();
    await popoverOpen();
    await shot('light-date-picker');
    await (await driver.findElement(By.css(`${scope} button[aria-label="Close calendar"]`))).click();
    await sleep(300);
    await (await driver.findElement(By.css(`${scope} button[type="submit"]`))).click();
    await pageHas('EJFS-2026-0042');
    await shot('light-submission-added');
  });

  await check('Attach and preview a PDF', async () => {
    await go(`#/papers/${paperId}?tab=files`);
    await clickText('Upload a file');
    await visible(By.css('dialog[data-modal="file-form"]'));
    await clickText('Choose a file');
    await pageHas('sample-manuscript.pdf');
    await shot('light-file-upload');
    await (await driver.findElement(By.css('dialog[data-modal="file-form"] button[type="submit"]'))).click();
    await visible(xpathText('button', 'Preview'));
    await shot('light-paper-files');
    await clickText('Preview');
    await sleep(2500);
    await shot('light-pdf-preview');
    await escape();
  });

  await check('Load the demo library', async () => {
    const r = await api('backup.import', { path: path.join(here, 'demo-backup.json') });
    if (r.error) throw new Error(JSON.stringify(r.error));
    await driver.navigate().refresh();
    await sleep(1500);
    await watchErrors();
  });

  const screens = [
    ['desk', '#/dashboard', 'Good '],
    ['papers', '#/papers', 'Papers'],
    ['paper-overview', null, 'Journey'],
    ['paper-files', null, 'Files'],
    ['conferences', '#/venues', 'Where your papers can go'],
    ['journals', '#/venues?tab=journal', 'Where your papers can go'],
    ['calls-for-papers', '#/opportunities', 'Calls that fit your work'],
    ['deadlines', '#/deadlines', 'Deadlines'],
    ['published', '#/published', 'Your published work'],
    ['settings-profile', '#/settings/profile', 'Settings'],
    ['settings-appearance', '#/settings/appearance', 'Settings'],
    ['settings-research', '#/settings/research', 'Settings'],
    ['settings-ai', '#/settings/ai', 'Settings'],
    ['settings-agents', '#/settings/agents', 'Settings'],
    ['settings-data', '#/settings/data', 'Settings'],
    ['settings-general', '#/settings/general', 'Settings'],
    ['settings-about', '#/settings/about', 'Settings'],
  ];

  for (const theme of ['light', 'dark']) {
    await check(`Theme ${theme}`, async () => {
      await setTheme(theme);
      await watchErrors();
    });
    for (const [name, hash, text] of screens) {
      await check(`${theme} ${name}`, async () => {
        if (name === 'paper-overview') {
          await go('#/papers');
          await (await byExactText('Designing Offline-First Health Records for Rural Clinics')).click();
        }
        else if (name === 'paper-files') await go(`#/papers/${paperId}?tab=files`);
        else await go(hash);
        await pageHas(text);
        if (name === 'settings-ai') {
          // The schedules of the automatic AI jobs sit further down the AI page.
          await driver.executeScript(`const el = [...document.querySelectorAll('h2,h3,[data-flux-heading]')].find((e) => /automatic/i.test(e.textContent)); if (el) el.scrollIntoView({ block: 'start' });`);
          await sleep(400);
        }
        await shot(`${theme}-${name}`);
      });
    }
    await check(`${theme} profile photo cropper`, async () => {
      await go('#/settings/profile');
      await clickText('Upload photo');
      await pageHas('Rotate 90°');
      await sleep(800);
      await shot(`${theme}-avatar-crop`);
      if (theme === 'light') {
        await clickText('Save', 'dialog[@open]//button');
        await sleep(1200);
        await shot('light-avatar-saved');
      } else {
        await clickText('Cancel', 'dialog[@open]//button');
        await sleep(500);
      }
    });
    await check(`${theme} new paper modal`, async () => {
      await go('#/dashboard');
      await clickText('New paper');
      await visible(By.css('dialog[data-modal="new-paper"] input[name="title"]'));
      await shot(`${theme}-new-paper`);
      await escape();
    });
    await check(`${theme} date picker`, async () => {
      await go(`#/papers/${paperId}`);
      await clickText('Add another');
      await (await visible(By.css('dialog[data-modal="sub-form"] button[aria-label="Open calendar"]'))).click();
      await popoverOpen();
      await shot(`${theme}-date-picker`);
      await escape();
      await escape();
    });
    await check(`${theme} pdf preview`, async () => {
      await go(`#/papers/${paperId}?tab=files`);
      await clickText('Preview');
      await sleep(2500);
      await shot(`${theme}-pdf-preview`);
      await escape();
    });
    await check(`${theme} notifications`, async () => {
      await go('#/dashboard');
      await clickText('Notifications');
      await sleep(600);
      await shot(`${theme}-notifications`);
      await escape();
    });
    await check(`${theme} command palette`, async () => {
      // Ctrl+K is a native menu accelerator, which WebDriver keys do not reach; on the Desk (no search field) “/”
      // opens the same palette.
      await go('#/dashboard');
      await driver.executeScript(`window.dispatchEvent(new KeyboardEvent('keydown', { key: '/', code: 'Slash', bubbles: true }))`);
      const input = await visible(By.css('[data-flux-command-palette] input'));
      await input.sendKeys('Health');
      await sleep(600);
      await shot(`${theme}-command-palette`);
      await escape();
    });
    await collectErrors();
  }

  await check('Backup export', async () => {
    await setTheme('light');
    await go('#/settings/data');
    // “Include files” is off for a JSON backup, which the test can read back.
    if (await driver.executeScript('return document.body.innerText.includes("Export .zip")')) {
      await (await visible(By.css('ui-checkbox[data-flux-checkbox][data-checked]'))).click();
      await pageHas('Export JSON');
    }
    await clickText('Export JSON');
    const dir = path.join(dataDir, 'exports');
    await driver.wait(() => fs.existsSync(dir) && fs.readdirSync(dir).some((f) => f.endsWith('.json')), 15000, 'No backup file was written');
    const file = fs.readdirSync(dir).find((f) => f.endsWith('.json'));
    const data = JSON.parse(fs.readFileSync(path.join(dir, file), 'utf8'));
    if (data.app !== 'manuscript-tracker' || !data.papers.some((p) => p.title.startsWith('Field Notes'))) {
      throw new Error('The backup does not contain the new paper');
    }
    await shot('light-backup-exported');
  });

  await collectErrors();
  if (pageErrors.length) {
    console.log('Page errors:\n  ' + pageErrors.join('\n  '));
    failures.push(`${pageErrors.length} page error(s)`);
  }
}

/** An RGB PNG with a soft diagonal gradient and a circle (fictional, generated). */
function makePng(w, h) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const i = y * (w * 3 + 1) + 1 + x * 3;
      const inCircle = (x - w / 2) ** 2 + (y - h / 2.4) ** 2 < (h / 4) ** 2;
      raw[i] = inCircle ? 250 : 60 + Math.round((140 * x) / w);
      raw[i + 1] = inCircle ? 214 : 70 + Math.round((60 * y) / h);
      raw[i + 2] = inCircle ? 160 : 200;
    }
  }
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/** A minimal one-page PDF with a line of text (valid cross-reference table). */
function makePdf(text) {
  const esc = text.replace(/[\\()]/g, (c) => '\\' + c);
  const content = `BT /F1 18 Tf 72 720 Td (${esc}) Tj 0 -28 Td /F1 12 Tf (Fictional sample used by the end-to-end tests.) Tj ET`;
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  let out = '%PDF-1.4\n';
  const offsets = [];
  objects.forEach((o, i) => {
    offsets.push(out.length);
    out += `${i + 1} 0 obj\n${o}\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const off of offsets) out += `${String(off).padStart(10, '0')} 00000 n \n`;
  out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}

let code = 0;
try {
  await tour();
} catch (e) {
  console.error(e);
  failures.push(e.message);
} finally {
  if (driver) await driver.quit().catch(() => {});
  tauriDriver.kill();
  fs.rmSync(dataDir, { recursive: true, force: true });
}
if (failures.length) {
  console.error(`\n${failures.length} step(s) failed:\n  ${failures.join('\n  ')}`);
  code = 1;
} else {
  console.log(`\nAll steps passed. Screenshots: ${shots}`);
}
process.exit(code);
