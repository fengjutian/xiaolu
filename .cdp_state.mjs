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
const response = await send("Runtime.evaluate", {
  expression: `(() => {
    const pres = [...document.querySelectorAll('pre.report')];
    const markdown = pres[0]?.innerText ?? '';
    const hostRow = markdown.split('\\n').find((line) => line.startsWith('| 主机名'));
    return {
      count: pres.length,
      head: markdown.slice(0, 160),
      hostRow: hostRow ?? '',
      hasHost: markdown.includes('C121041'),
      bodyTail: document.body.innerText.slice(-200),
      buttons: [...document.querySelectorAll('button')].map((b) => b.innerText.trim())
    };
  })()`,
  returnByValue: true,
});
console.log(JSON.stringify(response.result?.result?.value ?? response, null, 2));
socket.close();
