export function byData(root, selector) {
  return root.querySelector(`[data-field="${selector}"]`);
}

export function setText(node, value) {
  if (!node) return;
  node.textContent = value === null || value === undefined || value === "" ? "-" : String(value);
}

export function replaceList(list, items, renderItem) {
  if (!list) return;
  list.replaceChildren();
  if (!items.length) {
    const item = document.createElement("li");
    item.textContent = "No live records available";
    list.append(item);
    return;
  }
  for (const value of items) {
    const item = document.createElement("li");
    item.textContent = renderItem(value);
    list.append(item);
  }
}

export function setWarnings(node, warnings) {
  if (!node) return;
  node.replaceChildren();
  for (const warning of warnings) {
    const item = document.createElement("p");
    item.textContent = warning;
    node.append(item);
  }
}
