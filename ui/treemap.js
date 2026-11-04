"use strict";
// Treemap « squarified » à deux niveaux (dossiers + contenu), dessiné sur canvas.
const TM = (() => {
  const CATS = {
    video: [285, "Vidéo"], image: [165, "Images"], audio: [330, "Audio"], archive: [30, "Archives"],
    code: [212, "Code"], doc: [48, "Documents"], exe: [356, "Programmes"], other: [225, "Autres"],
  };
  const EXT = {};
  const add = (c, l) => l.split(" ").forEach(e => (EXT[e] = c));
  add("video", "mp4 mkv avi mov wmv webm m4v flv mpg mpeg");
  add("image", "jpg jpeg png gif bmp webp svg tif tiff heic raw psd ico cr2 nef arw dng");
  add("audio", "mp3 wav flac aac ogg m4a wma opus");
  add("archive", "zip rar 7z tar gz bz2 xz iso img vhd vhdx vmdk wim cab");
  add("code", "js ts jsx tsx py rs go java c cpp h cs html css json xml yml yaml toml md sh ps1 sql map lock rlib o obj pdb");
  add("doc", "pdf doc docx xls xlsx ppt pptx txt rtf csv odt epub");
  add("exe", "exe dll msi sys bat cmd apk jar");

  const cat = n => { const i = n.lastIndexOf("."); return i < 0 ? "other" : EXT[n.slice(i + 1).toLowerCase()] || "other"; };
  
  function squarify(sizes, x, y, w, h) {
    const total = sizes.reduce((a, b) => a + b, 0);
    const out = [];
    if (!total || w <= 0 || h <= 0) return sizes.map(() => [x, y, 0, 0]);
    const areas = sizes.map(s => (s * w * h) / total);
    const worst = (row, side) => {
      const s = row.reduce((a, b) => a + b, 0);
      return Math.max((side * side * Math.max(...row)) / (s * s), (s * s) / (side * side * Math.min(...row)));
    };
    let i = 0;
    while (i < areas.length) {
      const side = Math.max(Math.min(w, h), 1e-9);
      const row = [areas[i]];
      let j = i + 1;
      while (j < areas.length && worst([...row, areas[j]], side) <= worst(row, side)) row.push(areas[j++]);
      const s = row.reduce((a, b) => a + b, 0);
      if (w >= h) {
        const cw = s / h; let yy = y;
        for (const a of row) { out.push([x, yy, cw, a / cw]); yy += a / cw; }
        x += cw; w -= cw;
      } else {
        const rh = s / w; let xx = x;
        for (const a of row) { out.push([xx, y, a / rh, rh]); xx += a / rh; }
        y += rh; h -= rh;
      }
      i = j;
    }
    return out;
  }

  const HEAD = 18, PAD = 3;

  function layout(root, W, H) {
    const out = [];
    const place = (node, x, y, w, h, depth) => {
      const kids = (node.children || []).filter(c => c.size > 0).sort((a, b) => b.size - a.size);
      if (!kids.length) return;
      const rs = squarify(kids.map(k => k.size), x, y, w, h);
      kids.forEach((k, i) => {
        const [rx, ry, rw, rh] = rs[i];
        const r = { x: rx, y: ry, w: rw, h: rh, node: k, depth, head: 0 };
        out.push(r);
        if (k.is_dir && k.children && depth < 2 && rw > 96 && rh > 64) {
          r.head = HEAD;
          place(k, rx + PAD, ry + HEAD, rw - 2 * PAD, rh - HEAD - PAD, depth + 1);
        }
      });
    };
    place(root, 0, 0, W, H, 0);
    return out;
  }

  function draw(ctx, W, H, rects, ui) {
    const dark = ui.dark;
    ctx.clearRect(0, 0, W, H);
    ctx.textBaseline = "top";
    ctx.font = '12px "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif';
    const text = dark ? "#fff" : "#10131a";
    const labels = [];
    for (const r of rects) {
      const n = r.node;
      const x = r.x + 0.5, y = r.y + 0.5, w = r.w - 1, h = r.h - 1;
      if (w < 1 || h < 1) continue;
      // Les dossiers restent neutres ; la couleur est réservée au type de fichier (voir légende).
      let fill;
      if (!n.path) fill = dark ? "#2a2b30" : "#d4d5db";
      else if (n.is_dir) fill = r.head ? (dark ? "hsl(220 7% 21%)" : "hsl(220 10% 87%)") : (dark ? "hsl(220 8% 30%)" : "hsl(220 10% 77%)");
      else fill = `hsl(${CATS[cat(n.name)][0]} ${dark ? 34 : 45}% ${dark ? 40 : 72}%)`;
      ctx.fillStyle = fill;
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, Math.min(4, w / 4, h / 4));
      ctx.fill();
      if (ui.hover && ui.hover === n.path) {
        ctx.fillStyle = "rgba(255,255,255,.14)";
        ctx.fill();
      }
      if (w > 54 && h > 20) labels.push([r, x, y, w, h]);
    }
    for (const [r, x, y, w, h] of labels) {
      const n = r.node;
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, r.head || h);
      ctx.clip();
      ctx.fillStyle = dark && n.path && !n.is_dir ? "#fff" : text; // texte sombre sur fonds clairs
      ctx.font = (n.is_dir ? "600 " : "") + '12px "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif';
      ctx.fillText(n.name, x + 6, y + 4);
      if (!r.head && h > 38) {
        ctx.globalAlpha = 0.75;
        ctx.font = '11px "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif';
        ctx.fillText(ui.fmt(n.size), x + 6, y + 20);
      }
      ctx.restore();
    }
    for (const r of rects) {
      if (r.node.path && ui.sel.has(r.node.path)) {
        ctx.lineWidth = 2.5;
        ctx.strokeStyle = dark ? "#fff" : "#10131a";
        ctx.beginPath();
        ctx.roundRect(r.x + 1.5, r.y + 1.5, r.w - 3, r.h - 3, 3);
        ctx.stroke();
      }
    }
  }

  const hit = (rects, x, y) => {
    for (let i = rects.length - 1; i >= 0; i--) {
      const r = rects[i];
      if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) return r;
    }
    return null;
  };

  return { CATS, draw, layout, hit, cat };
})();
