// Decimal byte units (SI). Stored agent measurements remain raw bytes.
export const KB=1000,MB=KB**2,GB=KB**3,TB=KB**4;
export function formatBytes(value:unknown):string {
 if(typeof value!=='number'||!Number.isFinite(value)||value<0)return '—';
 const [scale,unit]=value>=TB?[TB,'TB']:value>=GB?[GB,'GB']:value>=MB?[MB,'MB']:value>=KB?[KB,'KB']:[1,'B'];
 return `${(value/(scale as number)).toLocaleString('ru-RU',{maximumFractionDigits:scale===1?0:1})} ${unit}`;
}
export const formatRate=(value:unknown)=>typeof value!=='number'||!Number.isFinite(value)||value<0?'—':formatBytes(value)+'/s';
