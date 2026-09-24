import {HardDrive,Info} from 'lucide-react';
import {Data} from './store';
import {formatBytes as size} from './units';
import './disk-capacity.css';

const colors:Record<string,string>={data:'#6fa6d5',system:'#a18bc8',vm:'#d4a35d',service:'#b77f9b',metadata:'#91a5b6',partitions:'#687388',free:'#61a991',used:'#6fa6d5'};
export function DiskCapacity({layout:l,disk}:{layout?:Data;disk?:Data}){
 const fresh=l?.ts&&Date.now()/1000-l.ts<180,available=fresh&&['available','partial'].includes(l?.status),total=available?l?.physical_total:null;
 const segments:Data[]=available?l?.segments||[]:[];
 return <div className="disk-capacity"><div className="disk-capacity-title"><div><HardDrive size={19}/><span>Весь физический SSD</span></div><strong>{size(total)}<small>полная ёмкость{l?.device?' · '+l.device:''}</small></strong></div>
  {available?<><div className="disk-capacity-stats"><span><b>{size(l?.used)}</b> занято в основном APFS</span><span><b>{size(l?.free)}</b> свободно</span><span>Контейнер APFS: <b>{size(l?.container_total)}</b></span></div><div className="disk-segments" role="img" aria-label={`SSD ${size(total)}: ${segments.map(s=>s.name+' '+size(s.size)).join(', ')}`}>{segments.filter(s=>s.size>0).map(s=><div key={s.id} style={{width:`${s.size/total*100}%`,background:colors[s.id]||'#8c93a1'}} title={`${s.name}: ${size(s.size)} · ${(s.size/total*100).toFixed(1)}%`}/>)}</div><div className="disk-segment-legend">{segments.filter(s=>s.size>0).map(s=><div key={s.id}><i style={{background:colors[s.id]||'#8c93a1'}}/><span>{s.name}</span><b>{size(s.size)}</b></div>)}</div><p><Info size={13}/><span>Шкала охватывает всю физическую ёмкость. Другие разделы и резерв не доступны для обычных файлов. Категория «Файлы и данные» разбирается по папкам ниже.</span></p></>:<div className="disk-capacity-fallback"><p>Физическая разметка пока недоступна. Ёмкость доступного тома APFS: <b>{size(disk?.total)}</b>, свободно <b>{size(disk?.free)}</b>.</p></div>}
 </div>;
}
