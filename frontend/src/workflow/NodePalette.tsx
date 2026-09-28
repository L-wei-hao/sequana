import { useState, type FC } from "react";
import type { NodeType } from "../types";
import { NODE_CATALOG } from "./nodeCatalog";

interface Props {
  onAddNode: (type: NodeType) => void;
}

export const NodePalette: FC<Props> = ({ onAddNode }) => {
  const [filter, setFilter] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string>("all");

  const categories = ["all", "trigger", "logic", "ai", "data", "action"];

  const filteredItems = NODE_CATALOG.filter((item) => {
    const matchesFilter =
      item.label.toLowerCase().includes(filter.toLowerCase()) ||
      item.description.toLowerCase().includes(filter.toLowerCase());
    const matchesCat =
      selectedCategory === "all" || item.category === selectedCategory;
    return matchesFilter && matchesCat;
  });

  return (
    <aside className="node-palette">
      <div className="palette-header">
        <h4>Nodes</h4>
        <input
          type="text"
          className="search-input"
          placeholder="Filter nodes..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>

      <div className="category-chips">
        {categories.map((cat) => (
          <button
            key={cat}
            className={`chip ${selectedCategory === cat ? "active" : ""}`}
            onClick={() => setSelectedCategory(cat)}
          >
            {cat.charAt(0).toUpperCase() + cat.slice(1)}
          </button>
        ))}
      </div>

      <div className="palette-items">
        {filteredItems.map((item) => (
          <button
            key={item.type}
            className={`palette-item ${item.category}`}
            onClick={() => onAddNode(item.type)}
          >
            <div className="palette-item-header">
              <span className="palette-item-icon">{item.label}</span>
              <span className={`tag tag-${item.category}`}>{item.category}</span>
            </div>
            <p className="palette-item-desc">{item.description}</p>
          </button>
        ))}
        {filteredItems.length === 0 && (
          <p className="muted empty-note">No matching nodes.</p>
        )}
      </div>
    </aside>
  );
};
