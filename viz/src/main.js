import { loadData } from './data.js';
import { computeLayout } from './layout.js';

async function init() {
  const status = document.getElementById('load-status');
  status.textContent = 'Loading data...';

  const data = await loadData();
  console.log(`Loaded: ${data.stats.files} files, ${data.stats.struct_edges} edges`);

  status.textContent = 'Computing layout...';
  const layout = computeLayout(data.tree);
  status.textContent = 'Ready.';
}

init().catch(err => console.error('Init failed:', err));
