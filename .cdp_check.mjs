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
const sensitive = await evaluate(`(() => {
  const markdown = [...document.querySelectorAll('pre.report')][0]?.innerText ?? '';
  const lines = markdown.split('\\n');
  const hits = lines.filter((line) => /SerialNumber|MACAddress|IPAddress|"HostName"/i.test(line));
  return { hits: hits.slice(0, 12), hasRedacted: markdown.includes('[已脱敏]') };
})()`);
console.log("SENSITIVE", JSON.stringify(sensitive, null, 2));

const toggled = await evaluate(`(() => {
  const label = [...document.querySelectorAll('label')].find((node) => node.innerText.includes('隐藏主机名'));
  if (!label) return 'label not found';
  label.click();
  return 'clicked';
})()`);
console.log("toggle", toggled);
await wait(500);
console.log(
  "generate",
  await evaluate(`(() => {
    const node = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === '生成预览');
    node?.click();
    return node ? 'clicked' : 'missing';
  })()`),
);
await wait(16000);
const after = await evaluate(`(() => {
  const markdown = [...document.querySelectorAll('pre.report')][0]?.innerText ?? '';
  return {
    hasHost: markdown.includes('C121041'),
    hostLine: markdown.split('\\n').find((line) => line.includes('主机名')) ?? '',
    firstSystemRow: markdown.split('\\n').find((line) => line.startsWith('| 主机名')) ?? ''
  };
})()`);
console.log("AFTER", JSON.stringify(after, null, 2));
socket.close();
