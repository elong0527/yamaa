// Trusted preflight infrastructure. This local provider requests one denied
// helper call and one allowed benchmark-runtime call, without a model API.
import http from 'node:http';

let count = 0;
const commands = [
  `node -e "require('fs').writeFileSync('/tmp/yamaa-foreign-runtime-ran', 'bad')"`,
  `${process.argv[2]} --version`,
];

http.createServer(async (req, res) => {
  let raw = '';
  for await (const chunk of req) raw += chunk;
  const body = JSON.parse(raw);
  // OpenCode also requests a session title, without tools. That request must
  // not consume a test command or look like a tool-enabled model response.
  const command = body.tools?.length ? commands[count++] : undefined;
  const delta = command
    ? {
        tool_calls: [{
          index: 0,
          id: `call_${count}`,
          type: 'function',
          function: {
            name: 'bash',
            arguments: JSON.stringify({
              command,
              description: 'Runtime permission probe',
            }),
          },
        }],
      }
    : { content: 'Permission probe complete.' };
  const chunk = (delta, reason) => ({
    id: `chatcmpl_${count}`,
    object: 'chat.completion.chunk',
    created: 1,
    model: 'probe',
    choices: [{ index: 0, delta, finish_reason: reason }],
  });
  res.writeHead(200, { 'Content-Type': 'text/event-stream' });
  res.write(`data: ${JSON.stringify(chunk(delta, null))}\n\n`);
  res.write(`data: ${JSON.stringify(chunk({}, command ? 'tool_calls' : 'stop'))}\n\n`);
  res.end('data: [DONE]\n\n');
}).listen(18765, '127.0.0.1');
