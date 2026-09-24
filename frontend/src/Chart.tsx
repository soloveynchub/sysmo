import {useEffect, useRef, useState} from 'react';
import {createPortal} from 'react-dom';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import {samples, getPath, Data} from './store';

export type Line = {key:string; label:string; color:string; scale?:number; warning?:number; critical?:number};
const chartFont = '11px system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
const number = new Intl.NumberFormat('ru-RU', {maximumFractionDigits:2});

export function Chart({lines, period=300, height=240, source}: {lines:Line[]; period?:number; height?:number; source?:Data[]}) {
  const host = useRef<HTMLDivElement>(null);
  const tooltip = useRef<HTMLDivElement>(null);
  const [error, setError] = useState('');
  const key = JSON.stringify(lines);
  useEffect(() => {
    if (!host.current || !tooltip.current) return;
    const mount = host.current, tip = tooltip.current;
    let disposed = false, history:Data[] = [], live:Data[] = samples.get();
    let inside = false;
    const abort = new AbortController();
    const alerts = lines.flatMap((line,index) => [
      ...(line.warning != null ? [{line,index,threshold:line.warning,color:'#dca350',label:'Высокий уровень'}] : []),
      ...(line.critical != null ? [{line,index,threshold:line.critical,color:'#e76571',label:'Пик нагрузки'}] : []),
    ]);
    const hide = () => { tip.hidden = true; tip.setAttribute('aria-hidden','true'); };
    const muted = () => getComputedStyle(mount).getPropertyValue('--muted').trim() || '#858a97';
    const show = (u:uPlot) => {
      const i = u.cursor.idx, left = u.cursor.left, top = u.cursor.top;
      if (!inside || i == null || left == null || top == null || left < 0 || top < 0 || !u.data[0].length) { hide(); return; }
      const bounds = u.over.getBoundingClientRect();
      if (left > bounds.width || top > bounds.height) {hide();return;}
      const title = document.createElement('div'); title.className = 'chart-tooltip-time';
      title.textContent = new Date(u.data[0][i]*1000).toLocaleString('ru-RU', {day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit',second:'2-digit'});
      const rows = lines.map((line,n) => {
        const row = document.createElement('div'); row.className = 'chart-tooltip-row';
        const dot = document.createElement('i'); dot.style.background = line.color;
        const label = document.createElement('span'); label.textContent = line.label;
        const value = document.createElement('strong'), sample = u.data[n+1][i];
        value.textContent = sample == null ? 'Нет данных' : number.format(sample);
        if (sample != null && line.critical != null && sample >= line.critical) value.className='critical';
        else if (sample != null && line.warning != null && sample >= line.warning) value.className='warning';
        row.append(dot,label,value); return row;
      });
      const peaks = alerts.flatMap((alert,j) => {
        const peak = u.data[lines.length+1+j]?.[i], avg = u.data[alert.index+1]?.[i];
        if (alert.threshold !== alert.line.critical || peak == null || peak === avg) return [];
        const row = document.createElement('div'); row.className='chart-tooltip-peak';
        row.textContent=`Пик · ${alert.line.label}: ${number.format(peak)}`; return [row];
      });
      tip.replaceChildren(title,...rows,...peaks);tip.hidden=false;tip.setAttribute('aria-hidden','false');
      const w=tip.offsetWidth, h=tip.offsetHeight, x=bounds.left+left, y=bounds.top+top;
      const px = x+18+w > window.innerWidth-10 ? x-w-18 : x+18;
      const py = y+18+h > window.innerHeight-10 ? y-h-18 : y+18;
      tip.style.left=`${Math.max(10,Math.min(px,window.innerWidth-w-10))}px`;
      tip.style.top=`${Math.max(10,Math.min(py,window.innerHeight-h-10))}px`;
    };
    const chart = new uPlot({
      width:Math.max(100,mount.clientWidth),height,padding:[12,period>=86400?50:28,0,8],
      series:[{label:'Время'},...lines.map(l=>({label:l.label,stroke:l.color,width:1.7,spanGaps:false})),...alerts.map(a=>({label:a.label,stroke:a.color,width:2.5,spanGaps:false,points:{show:true,size:4,stroke:a.color,fill:a.color}}))],
      scales:{x:{time:true}},legend:{show:false},cursor:{drag:{x:false,y:false}},
      axes:[
        {stroke:muted,grid:{show:false},font:chartFont,size:38,values:(_u,vals)=>vals.map(v=>new Date(v*1000).toLocaleString('ru-RU',period>=86400?{day:'2-digit',month:'2-digit',hour:'2-digit',minute:'2-digit'}:{hour:'2-digit',minute:'2-digit',second:'2-digit'})),space:period>=86400?130:85},
        {stroke:muted,grid:{stroke:'rgba(130,138,155,.12)',width:1},font:chartFont,size:48},
      ], hooks:{setCursor:[show]},
    },[[],...lines.map(()=>[]),...alerts.map(()=>[])] as uPlot.AlignedData,mount);
    const enter = () => {inside=true;show(chart);};
    const leave = () => {inside=false;hide();};
    const escape = (e:KeyboardEvent) => {if(e.key==='Escape') leave();};
    const outside = (e:PointerEvent) => {if(!mount.contains(e.target as Node))leave();};
    chart.over.addEventListener('pointerenter',enter);
    chart.over.addEventListener('pointerdown',enter);
    chart.over.addEventListener('pointermove',enter);
    chart.over.addEventListener('pointerleave',leave);
    chart.over.addEventListener('pointercancel',leave);
    window.addEventListener('scroll',leave,true);
    window.addEventListener('keydown',escape);
    document.addEventListener('pointerdown',outside);
    const draw = () => {
      if (disposed || document.hidden) return;
      const end=Date.now()/1000, points=new Map<number,Data>();
      if(source) source.forEach(v=>points.set(v.ts,v));
      else {history.forEach(v=>points.set(v.ts,v));live.forEach(v=>points.set(v.ts,v));}
      const data=[...points.values()].filter(v=>v.ts>=end-period).sort((a,b)=>a.ts-b.ts);
      const read=(p:Data,l:Line)=>{const v=p.metrics?(typeof p.metrics[l.key]==='number'?p.metrics[l.key]:p.metrics[l.key]?.avg):getPath(p,l.key);return v==null?null:v/(l.scale||1);};
      chart.setData([data.map(v=>v.ts),...lines.map(l=>data.map(v=>read(v,l))),...alerts.map(a=>data.map(v=>{const peak=v.metrics?.[a.line.key]?.max;const n=peak==null?read(v,a.line):peak/(a.line.scale||1);return n!=null&&n>=a.threshold?n:null;}))] as uPlot.AlignedData);
      if(!source)chart.setScale('x',{min:end-period,max:end});
      show(chart);
    };
    const fetchHistory=()=>{
      if(source){draw();return;}
      fetch(`/api/history?since=${Date.now()/1000-period}`,{signal:abort.signal}).then(r=>{if(!r.ok)throw Error();return r.json();}).then(d=>{history=d;setError('');draw();}).catch(e=>{if(e.name!=='AbortError')setError('История недоступна');});
    };
    fetchHistory();
    const unsub=samples.sub(()=>{live=samples.get();draw();});
    const resize=new ResizeObserver(()=>{leave();chart.setSize({width:Math.max(100,mount.clientWidth),height});});resize.observe(mount);
    const repaint=()=>chart.redraw(false,true);
    const theme=new MutationObserver(repaint);theme.observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});
    const scheme=matchMedia('(prefers-color-scheme: dark)');scheme.addEventListener('change',repaint);
    window.addEventListener('monitor-resume',fetchHistory);
    const timer=setInterval(fetchHistory,60000);draw();
    return()=>{
      disposed=true;abort.abort();hide();unsub();resize.disconnect();theme.disconnect();scheme.removeEventListener('change',repaint);clearInterval(timer);
      window.removeEventListener('monitor-resume',fetchHistory);window.removeEventListener('scroll',leave,true);window.removeEventListener('keydown',escape);document.removeEventListener('pointerdown',outside);
      chart.over.removeEventListener('pointerenter',enter);chart.over.removeEventListener('pointerdown',enter);chart.over.removeEventListener('pointermove',enter);chart.over.removeEventListener('pointerleave',leave);chart.over.removeEventListener('pointercancel',leave);chart.destroy();
    };
  },[key,period,height,source]);
  return <div className="chart"><div ref={host}/><div className="chart-caption">{error || 'Наведите на график или коснитесь его, чтобы увидеть значения'}</div><div className="legend">{lines.map(l=><span key={l.key}><i style={{background:l.color}}/>{l.label}</span>)}</div>{createPortal(<div ref={tooltip} className="chart-tooltip" role="tooltip" hidden aria-hidden="true"/>,document.body)}</div>;
}

export function Spark({metric,color='#6bafdf'}:{metric:string;color?:string}){const path=useRef<SVGPathElement>(null);useEffect(()=>{const draw=()=>{const data=samples.get().slice(-60).map(v=>getPath(v,metric));const valid=data.filter((v):v is number=>v!==null);if(!valid.length)return;const max=Math.max(...valid,1),min=Math.min(...valid,0);let pen=false;let d='';data.forEach((v,i)=>{if(v===null){pen=false;return}d+=`${pen?'L':'M'}${i/Math.max(1,data.length-1)*180},${37-(v-min)/(max-min)*30} `;pen=true});path.current?.setAttribute('d',d)};draw();return samples.sub(draw)},[metric]);return <svg className="spark" viewBox="0 0 180 40" preserveAspectRatio="none" aria-hidden="true"><path ref={path} fill="none" stroke={color} strokeWidth="1.8" vectorEffect="non-scaling-stroke"/></svg>}
