// Rebuild the saved local dashboard without committing tournament snapshots.
const fs = require('node:fs');
const path = require('node:path');
const modules = process.env.DASHBOARD_NODE_MODULES || path.join(__dirname, 'node_modules');
const esbuild = require(path.join(modules, 'esbuild'));
const output = path.resolve(process.argv[2]);
const file = name => path.join(output, name);
const oldSource = fs.readFileSync(file('TournamentApp.tsx'), 'utf8');
const data = {};
for (const key of ['D', 'R', 'ACTIVE_ONLY']) {
  const line = oldSource.split('\n').find(line => line.startsWith(`const ${key} = `));
  if (!line) throw Error(`Missing saved payload ${key}`);
  data[key] = JSON.parse(line.slice(`const ${key} = `.length, -1));
}
let template = fs.readFileSync(path.join(__dirname, 'TournamentApp.tsx'), 'utf8');
let publishedSource = template;
for (const key of Object.keys(data)) {
  template = template.replace(`__DASHBOARD_${key}__`, `{__dashboardData:"${key}"}`);
  publishedSource = publishedSource.replace(`__DASHBOARD_${key}__`, JSON.stringify(data[key]));
}
const shim = `import React from 'react';
const views=['ratings','scheduler','graph','generations','generation-strength','retirement','archive'];
const normalize=value=>views.includes(value)?value:value==='bracket'?'scheduler':value==='next'||value==='active-only'?'archive':'ratings';
export function useCanvasState(key, initial) {
 const [value,setValue]=React.useState(()=>{try{
  const hash=location.hash.slice(1);const saved=JSON.parse(localStorage.getItem('nnue-tree:'+key));
  return key==='view'?normalize(hash==='retirement-status'?'retirement':views.includes(hash)?hash:saved??initial):saved??initial;
 }catch{return initial}});
 React.useEffect(()=>{if(key==='view')document.documentElement.dataset.dashboardView=normalize(value)},[key,value]);
 React.useEffect(()=>{if(key!=='view')return; const listener=()=>{const hash=location.hash.slice(1);if(hash==='retirement-status'||views.includes(hash))setValue(normalize(hash==='retirement-status'?'retirement':hash));};window.addEventListener('hashchange',listener);return()=>window.removeEventListener('hashchange',listener)},[key]);
 return [value,next=>{setValue(next);try{localStorage.setItem('nnue-tree:'+key,JSON.stringify(next));if(key==='view'){document.documentElement.dataset.dashboardView=normalize(next);history.replaceState(null,'','#'+next);}}catch{}}];
}
export const useHostTheme=()=>({text:{primary:'var(--fg)',secondary:'var(--muted)',tertiary:'var(--muted)',link:'var(--blue)'},bg:{editor:'var(--bg)',elevated:'var(--panel)'},stroke:{primary:'var(--line)',secondary:'var(--line)'},fill:{secondary:'var(--selected)'},accent:{primary:'var(--blue)'}});
export const Stack=({gap=16,style,children})=><div style={{display:'flex',flexDirection:'column',gap,...style}}>{children}</div>;
export const Row=({gap=12,style,children})=><div style={{display:'flex',gap,alignItems:'center',flexWrap:'wrap',...style}}>{children}</div>;
export const H1=({children})=><h1>{children}</h1>;export const H2=({children})=><h2>{children}</h2>;
export const Text=({children})=><p>{children}</p>;
export const Button=props=><button {...props}/>;
export const Select=({options,value,onChange})=><select value={value} onChange={e=>onChange(e.target.value)}>{options.map(o=><option key={o.value} value={o.value}>{o.label}</option>)}</select>;`;
const plugin={name:'saved-dashboard',setup(build){
 build.onResolve({filter:/^dashboard-app$/},()=>({path:'app',namespace:'dashboard'}));
 build.onResolve({filter:/^cursor\/canvas$/},()=>({path:'shim',namespace:'dashboard'}));
 build.onLoad({filter:/.*/,namespace:'dashboard'},args=>({contents:args.path==='app'?template:shim,loader:'tsx',resolveDir:path.dirname(modules)}));
}};
(async()=>{
 const result=await esbuild.build({stdin:{contents:"import {createRoot} from 'react-dom/client';import App from 'dashboard-app';createRoot(document.getElementById('root')).render(<App/>);",loader:'jsx',resolveDir:path.dirname(modules)},bundle:true,write:false,plugins:[plugin],nodePaths:[modules],jsx:'automatic',minify:true,format:'iife',define:{'process.env.NODE_ENV':'"production"'}});
 let js=result.outputFiles[0].text;
 for(const [key,value] of Object.entries(data)) {
  const placeholder=`{__dashboardData:"${key}"}`;
  if(js.split(placeholder).length!==2)throw Error(`Missing/ambiguous compiled data anchor ${key}`);
  js=js.replace(placeholder,`/*nnue-dashboard:${key}*/`+JSON.stringify(value));
 }
 let html=fs.readFileSync(file('nnue-tournament-react.html'),'utf8');
 for(const [from,to] of [['nnue-tournament-react.html','dashboard-archive-20261112.html'],['generation-strength.html','generation-strength-study-20261112.html']]){
  if(!fs.existsSync(file(to)))fs.copyFileSync(file(from),file(to));
 }
 const start=html.indexOf('<script>',html.indexOf('<div id="root"></div>'));
 const end=html.indexOf('</script>',start)+9;
 if(start<0||end<9)throw Error('Missing application script');
 html=html.slice(0,start)+'<script>'+js.replace(/<\/script/gi,'<\\/script')+'</script>'+html.slice(end);
 html=html.replace(/<!-- SEEDS2_CONFIDENCE_START -->[\s\S]*?<!-- SEEDS2_CONFIDENCE_END -->/,'');
 html=html.replace(/<aside id="rules-epoch-note"[\s\S]*?<\/aside>/,'');
 html=html.replace('<title>NNUE tournament tree</title>','<title>Tournament dashboard</title>');
 const extra='<style id="dashboard-layout">html:not([data-dashboard-view="retirement"]) #retirement-status{display:none}button[aria-pressed="true"]{background:var(--selected);border-color:var(--blue);font-weight:600}</style>';
 html=html.replace(/<style id="dashboard-layout">[\s\S]*?<\/style>/,'').replace('</head>',extra+'</head>');
 for(const key of Object.keys(data))if(html.split(`/*nnue-dashboard:${key}*/`).length!==2)throw Error('Lost refresh anchor');
 fs.writeFileSync(file('TournamentApp.tsx'),publishedSource);
 fs.writeFileSync(file('nnue-tournament-react.html'),html);
 fs.copyFileSync(path.join(__dirname,'generation-strength.html'),file('generation-strength.html'));
 console.log('Built dashboard; archived old views and retained live payloads/retirement table.');
})();
