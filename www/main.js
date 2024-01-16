// Live view

const canvas = document.querySelector('canvas')
const ctx = canvas.getContext('2d');

const url = `ws://${location.host}/liveview`;
const ws = new WebSocket(url);
ws.addEventListener('message', (ev) => {
    console.log(ev.data);
    const imgSrc = URL.createObjectURL(ev.data);
    const imgEl = document.createElement('img');
    imgEl.addEventListener('load', () => {
        canvas.width = imgEl.naturalWidth
        canvas.height = imgEl.naturalHeight
        ctx.drawImage(imgEl, 0, 0);
        URL.revokeObjectURL(imgSrc);
    }, { once: true });
    imgEl.src = imgSrc;
});

