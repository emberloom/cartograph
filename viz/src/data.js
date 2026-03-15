/**
 * Fetch and validate data.json.
 * Shows error overlay if missing or malformed.
 */
export async function loadData() {
  try {
    const resp = await fetch('./data.json');
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    const data = await resp.json();

    // Minimal schema validation
    if (!data.tree || !data.struct_edges || !data.stats) {
      throw new Error('Invalid data.json: missing required fields (tree, struct_edges, stats)');
    }

    return data;
  } catch (err) {
    const overlay = document.getElementById('error-overlay');
    const detail = document.getElementById('error-detail');
    overlay.classList.add('visible');
    detail.textContent = err.message;
    document.getElementById('loading-overlay').style.display = 'none';
    throw err;
  }
}
