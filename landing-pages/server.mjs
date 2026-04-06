import { createServer } from 'http';
import { readFile } from 'fs/promises';
import { join, extname } from 'path';
import { fileURLToPath } from 'url';

const dir = fileURLToPath(new URL('.', import.meta.url));
const port = process.env.PORT || 3000;

const mime = { '.html': 'text/html', '.css': 'text/css', '.js': 'application/javascript', '.png': 'image/png', '.svg': 'image/svg+xml' };

createServer(async (req, res) => {
  const path = req.url === '/' ? '/index.html' : req.url;
  try {
    const data = await readFile(join(dir, path));
    res.writeHead(200, { 'Content-Type': mime[extname(path)] || 'text/html' });
    res.end(data);
  } catch {
    res.writeHead(302, { Location: '/' });
    res.end();
  }
}).listen(port, () => console.log(`Listening on :${port}`));
