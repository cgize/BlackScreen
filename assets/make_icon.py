"""Genera assets/blackscreen.ico: un monitor con la pantalla en negro.

Uso: python assets/make_icon.py   (requiere Pillow)
Cada tamano se dibuja a 8x y se reduce, y se guarda como PNG de paleta
dentro del .ico (Windows Vista+ acepta PNG en cualquier tamano).
"""
import io, struct
from pathlib import Path
from PIL import Image, ImageDraw

SIZES = [16, 24, 32, 48, 256]
BEZEL = (176, 182, 190, 255)   # gris claro: visible en tema claro y oscuro
SCREEN = (0, 0, 0, 255)

def draw(size):
    s = 8
    n = size * s
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    u = n / 16  # unidad de la rejilla 16x16; bordes en enteros = nitido a 16 px

    def box(x0, y0, x1, y1, r, fill):
        d.rounded_rectangle([x0*u, y0*u, x1*u - 1, y1*u - 1], radius=r*u, fill=fill)

    box(0, 2, 16, 12, 1.5, BEZEL)   # marco del monitor
    box(1, 3, 15, 11, 0.5, SCREEN)  # pantalla negra
    box(7, 12, 9, 14, 0, BEZEL)     # cuello del pie
    box(4, 14, 12, 15, 0.5, BEZEL)  # base
    # BOX = media exacta de cada bloque 8x8: sin halos de remuestreo.
    return img.resize((size, size), Image.BOX)

LEVELS = 5  # niveles de suavizado por canal: bordes limpios con paleta minima

def png_bytes(img):
    # El dibujo solo mezcla gris y negro con distintas opacidades, asi que cada
    # pixel se reduce a (mezcla gris/negro, alfa) cuantizados: paleta de pocos
    # colores exactos, sin los tintes que mete un cuantizador generico.
    q = lambda x: round(min(max(x, 0.0), 1.0) * (LEVELS - 1)) / (LEVELS - 1)
    palette, index = [], {}
    pal = Image.new("P", img.size)
    px = pal.load()
    src = img.load()
    for y in range(img.height):
        for x in range(img.width):
            r, _, _, a = src[x, y]
            a = q(a / 255)
            t = q(r / BEZEL[0]) if a else 0.0
            key = (t, a)
            if key not in index:
                index[key] = len(palette)
                palette.append(key)
            px[x, y] = index[key]
    flat, alpha = [], []
    for t, a in palette:
        flat += [round(c * t) for c in BEZEL[:3]]
        alpha.append(round(a * 255))
    pal.putpalette(flat)
    buf = io.BytesIO()
    pal.save(buf, "PNG", optimize=True, transparency=bytes(alpha))
    return buf.getvalue()

def main():
    images = [png_bytes(draw(sz)) for sz in SIZES]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for sz, data in zip(SIZES, images):
        dim = 0 if sz >= 256 else sz
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(images)
    path = Path(__file__).with_name("blackscreen.ico")
    path.write_bytes(out)
    print(f"{path}: {len(out)} bytes", [len(i) for i in images])

main()
