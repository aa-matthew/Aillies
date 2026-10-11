// Reuse the supplied artwork exactly; Tauri converts it to platform icon formats.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {execFileSync} from 'node:child_process';
const brand='assets/brand', icons='apps/desktop/src-tauri/icons';
const mark=`data:image/png;base64,${fs.readFileSync(`${brand}/source/mark.png`).toString('base64')}`;
const wordmark=`data:image/png;base64,${fs.readFileSync(`${brand}/source/wordmark.png`).toString('base64')}`;
const svg=(body,view='0 0 1024 1024')=>`<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="${view}" role="img" aria-label="Aillies">${body}</svg>\n`;
const image=`<image x="64" y="64" width="896" height="896" xlink:href="${mark}"/>`;
fs.writeFileSync(`${brand}/logo.svg`,svg(`<rect width="1024" height="1024" rx="228" fill="#07101f"/>${image}`));
fs.writeFileSync(`${brand}/mark.svg`,svg(image));
fs.writeFileSync(`${brand}/wordmark.svg`,svg(`<image width="2172" height="724" xlink:href="${wordmark}"/>`,'0 0 2172 724'));
fs.copyFileSync(`${brand}/source/wordmark.png`,`${brand}/wordmark.png`);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'aillies-brand-'));
try {
 execFileSync('cargo',['tauri','icon',`${brand}/logo.svg`,'--output',temp],{stdio:'inherit'});
 for (const name of ['32x32.png','128x128.png','128x128@2x.png','icon.icns','icon.ico']) fs.copyFileSync(path.join(temp,name),`${icons}/${name}`);
 for(const [name,color] of Object.entries({idle:'#8e8e93',ready:'#30b877',action:'#e59a42'})) {
  // Alpha mask preserves the mark silhouette while using the existing status colors.
  const source=path.join(temp,`${name}.svg`);
  fs.writeFileSync(source,svg(`<defs><mask id="mark" mask-type="alpha">${image}</mask></defs><rect width="1024" height="1024" fill="${color}" mask="url(#mark)"/>`));
  execFileSync('cargo',['tauri','icon',source,'--output',temp,'--png','44'],{stdio:'inherit'});
  fs.copyFileSync(path.join(temp,'44x44.png'),`${icons}/tray-${name}@2x.png`);
 }
 execFileSync('cargo',['tauri','icon',`${brand}/logo.svg`,'--output',temp,'--png','1024,128'],{stdio:'inherit'});
 fs.copyFileSync(path.join(temp,'1024x1024.png'),`${brand}/logo-1024.png`);
 fs.copyFileSync(path.join(temp,'128x128.png'),'apps/desktop/ui/logo.png');
} finally { fs.rmSync(temp,{recursive:true,force:true}); }
