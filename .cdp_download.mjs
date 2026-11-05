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

await send("Page.enable");
const behavior = await send("Page.setDownloadBehavior", {
  behavior: "allow",
  downloadPath: "D:\\github\\xiaolu\\.tmp_downloads",
});
console.log("behavior", JSON.stringify(behavior));
console.log(
  "click",
  await evaluate(`(() => {
    const nodes = [...document.querySelectorAll('button')].filter((item) => item.innerText.trim() === '瀵煎嚭 JSON');
    if (nodes.length === 0) return 'missing';
    nodes[nodes.length - 1].click();
    return 'clicked';
  })()`),
);
await wait(3000);
console.log(
  "buttons",
  await evaluate(`([...document.querySelectorAll('button')].map((item) => item.innerText.trim()))`),
);
socket.close();
