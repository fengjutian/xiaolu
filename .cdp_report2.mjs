const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const targets = await (await fetch("http://127.0.0.1:9333/json")).json();
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
await wait(5000);
console.log(
  "nav",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('a,button')].find((item) => item.innerText.trim() === '诊断报告');
    node?.click();
    return node ? 'clicked' : 'missing';
  })()`),
);
await wait(800);
console.log(
  "generate",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '生成预览');
    node?.click();
    return node ? 'clicked' : 'missing';
  })()`),
);
for (let attempt = 0; attempt < 20; attempt += 1) {
  await wait(2000);
  const ready = await evaluate(`document.body.innerText.includes('字段预览')`);
  if (ready) break;
}
const first = await evaluate(`(() => {
  const markdown = [...document.querySelectorAll('pre.report')][0]?.innerText ?? '';
  return {
    hasTasks: markdown.includes('## 计划任务'),
    tasksLine: markdown.split('\\n').find((line) => line.includes('共 202 个任务')) ?? '',
    hasCleanup: markdown.includes('## 清理候选'),
    schema: markdown.split('\\n').find((line) => line.includes('schemaVersion')) ?? 'n/a'
  };
})()`);
console.log("FIRST", JSON.stringify(first, null, 2));

console.log(
  "toggleCleanup",
  await evaluate(`(() => {
    const label = [...document.querySelectorAll('label')].find((node) => node.innerText.includes('清理候选（较慢）'));
    if (!label) return 'label not found';
    label.click();
    return 'clicked';
  })()`),
);
await wait(500);
console.log(
  "regen",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '生成预览');
    node?.click();
    return node ? 'clicked' : 'missing';
  })()`),
);
for (let attempt = 0; attempt < 40; attempt += 1) {
  await wait(2000);
  const ready = await evaluate(`(() => {
    const markdown = [...document.querySelectorAll('pre.report')][0]?.innerText ?? '';
    return markdown.includes('## 清理候选') || [...document.querySelectorAll('[role="alert"]')].some((node) => node.innerText.includes('失败'));
  })()`);
  if (ready) break;
}
const second = await evaluate(`(() => {
  const markdown = [...document.querySelectorAll('pre.report')][0]?.innerText ?? '';
  const cleanupIndex = markdown.indexOf('## 清理候选');
  return {
    hasTasks: markdown.includes('## 计划任务'),
    hasCleanup: cleanupIndex >= 0,
    cleanupSnippet: cleanupIndex >= 0 ? markdown.slice(cleanupIndex, cleanupIndex + 500) : '',
    totalLine: markdown.split('\\n').find((line) => line.includes('合计 ')) ?? ''
  };
})()`);
console.log("SECOND", JSON.stringify(second, null, 2));
socket.close();
