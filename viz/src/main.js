import { loadData } from './data.js';

async function init() {
  const status = document.getElementById('load-status');
  status.textContent = 'Loading data...';

  const data = await loadData();
  console.log(`Loaded: ${data.stats.files} files, ${data.stats.struct_edges} edges`);

  status.textContent = 'Data loaded.';
  // Layout, renderer, interaction, UI will be wired in subsequent tasks.
}

init().catch(err => console.error('Init failed:', err));
