const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find((item) => item.type === "page");
if (!target) {
  console.log("no page target");
  process.exit(1);
}
const socket = new WebSocket(target.webSocketDebuggerUrl);
let nextId = 0;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => socket.addEventListener("open", resolve, { once: true }));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++nextId;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const response = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  return response.result?.result?.value ?? response.result?.exceptionDetails;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url: "http://127.0.0.1:1420/" });
await wait(4000);
console.log(
  "nav",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('a,button')].find((item) => item.innerText.trim() === '诊断报告');
    if (!node) return 'missing';
    node.click();
    return 'clicked';
  })()`),
);
await wait(800);
console.log(
  "generate",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '生成预览');
    if (!node) return 'missing';
    node.click();
    return 'clicked';
  })()`),
);

for (let attempt = 0; attempt < 20; attempt += 1) {
  await wait(2000);
  const state = await evaluate(`({
    fields: (document.body.innerText.match(/个字段/g) ?? []).length,
    hasPreview: document.body.innerText.includes('字段预览'),
    loading: [...document.querySelectorAll('button')].some((item) => item.innerText.includes('生成预览') && item.getAttribute('data-loading') === 'true'),
    buttonTexts: [...document.querySelectorAll('button')].map((item) => item.innerText.trim()).filter(Boolean).slice(0, 10)
  })`);
  if (state?.hasPreview) break;
}

const result = await evaluate(`(() => {
  const pres = [...document.querySelectorAll('pre.report')].map((node) => node.innerText);
  const text = document.body.innerText;
  return {
    preCount: pres.length,
    markdown: (pres[0] ?? '').slice(0, 900),
    jsonHead: (pres[1] ?? '').slice(0, 300),
    markdownHasHost: (pres[0] ?? '').includes('C121041'),
    markdownHasRedacted: (pres[0] ?? '').includes('[已脱敏]'),
    jsonHasPid: /"pid"\\s*:/.test(pres[1] ?? ''),
    buttons: [...document.querySelectorAll('button')].map((item) => item.innerText.trim()).filter(Boolean),
    errors: [...document.querySelectorAll('[role="alert"]')].map((item) => item.innerText.slice(0, 200))
  };
})()`);
console.log(JSON.stringify(result, null, 2));
socket.close();
