const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find((item) => item.type === "page");
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
await wait(4500);
console.log(
  "nav",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('a,button')].find((item) => item.innerText.trim() === '空间分析');
    if (!node) return 'missing';
    node.click();
    return 'clicked';
  })()`),
);
await wait(1000);
console.log(
  "large",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '扫描大文件');
    if (!node) return 'missing';
    node.click();
    return 'clicked';
  })()`),
);

for (let attempt = 0; attempt < 15; attempt += 1) {
  await wait(2000);
  const ready = await evaluate(`document.body.innerText.includes('最大的文件') || [...document.querySelectorAll('[role="alert"]')].some((node) => node.innerText.includes('失败') || node.innerText.includes('不存在'))`);
  if (ready) break;
}
const largeState = await evaluate(`(() => {
  const text = document.body.innerText;
  return {
    hasTable: text.includes('最大的文件'),
    summary: (text.match(/\\d+ 个文件 · 总计 [^\\n]+/) ?? [''])[0],
    topDirs: [...document.querySelectorAll('table')].length,
    alerts: [...document.querySelectorAll('[role="alert"]')].map((node) => node.innerText.slice(0, 160))
  };
})()`);
console.log("LARGE", JSON.stringify(largeState, null, 2));

console.log(
  "dupes",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '查找重复文件');
    if (!node) return 'missing';
    node.click();
    return 'clicked';
  })()`),
);
for (let attempt = 0; attempt < 45; attempt += 1) {
  await wait(2000);
  const ready = await evaluate(`document.body.innerText.includes('组重复') || document.body.innerText.includes('未发现重复文件')`);
  if (ready) break;
}
const dupeState = await evaluate(`(() => {
  const text = document.body.innerText;
  return {
    hasSummary: text.includes('组重复'),
    summary: (text.match(/已哈希 [^\\n]+/) ?? [''])[0],
    empty: text.includes('未发现重复文件'),
    firstGroup: (text.match(/\\d+ 个相同文件 · 每个 [^\\n]+/) ?? [''])[0],
    alerts: [...document.querySelectorAll('[role="alert"]')].map((node) => node.innerText.slice(0, 200))
  };
})()`);
console.log("DUPES", JSON.stringify(dupeState, null, 2));
socket.close();
