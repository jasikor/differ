// Rails and connectors, drawn into an SVG overlay on top of the laid-out rows.
//
// Rust emits the rows and a block list (row ranges per side); everything here
// is geometry. Rails run along the inner edge of each pane, and a cubic curve
// joins the two rails across the gutter. A block with only one side ends in a
// dashed stub and a hollow dot: these lines have no counterpart to reach.
(function () {
  const SVG = "http://www.w3.org/2000/svg";
  const DOCS = window.DOCS || [];
  const RAIL_INSET = 10; // matches --rail-inset
  const END_INSET = 3; // rail cap inset inside the first/last row

  function el(name, attrs) {
    const node = document.createElementNS(SVG, name);
    for (const key in attrs) node.setAttribute(key, attrs[key]);
    return node;
  }

  // Row top/bottom offsets relative to the body, indexed by row number.
  function rowOffsets(pane, top) {
    const out = [];
    for (const row of pane.querySelectorAll(".row")) {
      const r = row.getBoundingClientRect();
      out[Number(row.dataset.row)] = [r.top - top, r.bottom - top];
    }
    return out;
  }

  function draw(body, doc) {
    const svg = body.querySelector("svg.rails");
    if (!svg) return;
    const oldPane = body.querySelector(".pane.old");
    const newPane = body.querySelector(".pane.new");
    if (!oldPane || !newPane) return;

    const box = body.getBoundingClientRect();
    const oldX = oldPane.getBoundingClientRect().right - box.left - RAIL_INSET;
    const newX = newPane.getBoundingClientRect().left - box.left + RAIL_INSET;
    const oldY = rowOffsets(oldPane, box.top);
    const newY = rowOffsets(newPane, box.top);

    svg.replaceChildren();
    const file = body.dataset.diff;

    for (let i = 0; i < doc.blocks.length; i++) {
      const block = doc.blocks[i];
      const g = el("g", { class: "blk " + block.k });
      g.id = "blk-" + file + "-" + i;

      const old = block.o && span(oldY, block.o);
      const added = block.n && span(newY, block.n);

      if (old) {
        g.appendChild(
          el("path", {
            class: "rail",
            d: "M " + oldX + " " + old[0] + " L " + oldX + " " + old[1],
          })
        );
      }
      if (added) {
        g.appendChild(
          el("path", {
            class: "rail",
            d: "M " + newX + " " + added[0] + " L " + newX + " " + added[1],
          })
        );
      }

      if (old && added) {
        const oy = (old[0] + old[1]) / 2;
        const ny = (added[0] + added[1]) / 2;
        const dx = (newX - oldX) * 0.42;
        g.appendChild(
          el("path", {
            class: "link",
            d:
              "M " + oldX + " " + oy +
              " C " + (oldX + dx) + " " + oy +
              ", " + (newX - dx) + " " + ny +
              ", " + newX + " " + ny,
          })
        );
      } else if (old) {
        stub(g, oldX, old[0], old[1], oldX + (newX - oldX) * 0.34, 1);
      } else if (added) {
        stub(g, newX, added[0], added[1], newX - (newX - oldX) * 0.34, -1);
      }

      svg.appendChild(g);
    }
  }

  // The visible extent of a rail: the block's rows, inset from the caps.
  function span(rows, range) {
    const first = rows[range[0]];
    const last = rows[range[1] - 1];
    if (!first || !last) return null;
    return [first[0] + END_INSET, last[1] - END_INSET];
  }

  // A block with no counterpart: a short dashed lead-in and a hollow end dot.
  function stub(g, x, top, bottom, endX, dir) {
    const y = (top + bottom) / 2;
    g.appendChild(
      el("path", { class: "stub", d: "M " + x + " " + y + " L " + endX + " " + y })
    );
    g.appendChild(el("circle", { class: "end", cx: endX, cy: y, r: 3.5 }));
  }

  // Wrapped lines can make one pane's row taller than its twin. Give both the
  // same height so the rows stay paired and the rails line up.
  function equalize(body) {
    const oldRows = body.querySelectorAll(".pane.old .row");
    const newRows = body.querySelectorAll(".pane.new .row");
    oldRows.forEach((row) => (row.style.height = ""));
    newRows.forEach((row) => (row.style.height = ""));
    const count = Math.min(oldRows.length, newRows.length);
    for (let i = 0; i < count; i++) {
      const height = Math.max(oldRows[i].offsetHeight, newRows[i].offsetHeight);
      oldRows[i].style.height = height + "px";
      newRows[i].style.height = height + "px";
    }
  }

  function drawAll() {
    const bodies = document.querySelectorAll(".body[data-diff]");
    bodies.forEach((body) => {
      equalize(body);
      const doc = DOCS[Number(body.dataset.diff)];
      if (doc) draw(body, doc);
    });
  }

  // Spotlight: hovering any changed row brightens its rails and dims the rest.
  function clearHover() {
    document
      .querySelectorAll(".body.hovering")
      .forEach((b) => b.classList.remove("hovering"));
    document.querySelectorAll(".blk.hot").forEach((b) => b.classList.remove("hot"));
  }

  document.addEventListener("mouseover", (event) => {
    const row = event.target.closest && event.target.closest(".row[data-block]");
    if (!row) {
      const inside = event.target.closest && event.target.closest(".body");
      if (!inside) clearHover();
      return;
    }
    const body = row.closest(".body");
    const target = document.getElementById(row.dataset.block);
    if (!body || !target) return;
    document.querySelectorAll(".blk.hot").forEach((b) => b.classList.remove("hot"));
    document.querySelectorAll(".body.hovering").forEach((b) => b.classList.remove("hovering"));
    body.classList.add("hovering");
    target.classList.add("hot");
  });

  let pending = false;
  function schedule() {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      drawAll();
    });
  }

  window.addEventListener("resize", schedule);
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(schedule);
  drawAll();
})();
