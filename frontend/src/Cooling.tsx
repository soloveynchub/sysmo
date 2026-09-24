import {useEffect, useId, useRef, useState} from 'react';
import {Fan, Pause, Play, Wind} from 'lucide-react';
import {connection, Data, system, useChannel} from './store';

// 60 physical revolutions are displayed as one visual revolution.
// Web Animations preserves rotation phase when a new RPM sample arrives.
function FanScene({fan, index, running, paused, temperature}: {
  fan: Data; index: number; running: boolean; paused: boolean; temperature: number | null;
}) {
  const id = useId().replace(/:/g, '');
  const scene = useRef<HTMLDivElement>(null);
  const rotor = useRef<SVGGElement>(null);
  const flow = useRef<SVGGElement>(null);
  const animations = useRef<Animation[]>([]);
  const [visible, setVisible] = useState(false);
  const [reduced, setReduced] = useState(false);
  const rpm = running && Number.isFinite(fan.rpm) && fan.rpm >= 0 ? fan.rpm : null;
  const max = Number.isFinite(fan.max_rpm) && fan.max_rpm > 0 ? fan.max_rpm : null;
  const ratio = rpm != null && max ? Math.min(1, rpm / max) : null;
  const moving = rpm != null && rpm > 0 && !paused && !reduced && visible;
  useEffect(() => {
    const mq = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(mq.matches);
    update(); mq.addEventListener('change', update);
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting));
    if (scene.current) observer.observe(scene.current);
    return () => { mq.removeEventListener('change', update); observer.disconnect(); };
  }, []);
  useEffect(() => {
    const spin = rotor.current!.animate([{transform:'rotate(0deg)'}, {transform:'rotate(360deg)'}], {duration:1200, iterations:Infinity});
    const air = flow.current!.animate([{strokeDashoffset:0}, {strokeDashoffset:-120}], {duration:1800, iterations:Infinity});
    animations.current = [spin, air];
    spin.pause(); air.pause();
    return () => { spin.cancel(); air.cancel(); animations.current = []; };
  }, []);
  useEffect(() => {
    const update = () => animations.current.forEach((animation, i) => {
      animation.updatePlaybackRate(i === 0 ? (rpm || 0) / 3000 : (rpm || 0) / 4000);
      if (moving && !document.hidden) animation.play(); else animation.pause();
    });
    update(); document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  }, [moving, rpm]);
  const status = rpm == null ? 'Нет свежих данных' : rpm === 0 ? 'Остановлен' : 'Работает';
  return <div className="cooling-unit cinematic" ref={scene}>
    <div className="cooling-stage" data-moving={moving}>
      <div className="cooling-stage-label"><span className={rpm == null ? 'offline' : 'online'}/>{status}<span>APPLE M3 · FAN {index + 1}</span></div>
      <svg viewBox="0 0 1200 600" className="cooling-scene" role="img" aria-label={`Внутри MacBook. Вентилятор ${index+1}: ${rpm == null ? 'нет данных' : `${Math.round(rpm)} оборотов в минуту`}. Вращение замедлено в 60 раз.`}>
        <defs>
          <radialGradient id={`${id}-well`}><stop stopColor="#1b2229"/><stop offset="1" stopColor="#030506"/></radialGradient>
          <linearGradient id={`${id}-blade`} x2=".7" y2="1"><stop stopColor="#11191e"/><stop offset=".4" stopColor="#59616a"/><stop offset=".55" stopColor="#263037"/><stop offset="1" stopColor="#080d11"/></linearGradient>
          <radialGradient id={`${id}-hub`} cx=".32" cy=".2"><stop stopColor="#6b747c"/><stop offset=".4" stopColor="#2f373e"/><stop offset="1" stopColor="#0a0f13"/></radialGradient>
          <linearGradient id={`${id}-fade`} x1="0" y1="0" x2="0" y2="1"><stop offset=".45" stopColor="#070b10" stopOpacity="0"/><stop offset=".82" stopColor="#070b10" stopOpacity=".8"/><stop offset="1" stopColor="#070b10"/></linearGradient>
          <linearGradient id={`${id}-cold`} x1="0" y1="1" x2="1" y2="0"><stop stopColor="#64b7ff" stopOpacity="0"/><stop offset=".35" stopColor="#64b7ff" stopOpacity=".65"/><stop offset=".8" stopColor="#aee5ff"/><stop offset="1" stopColor="#d7f7ff" stopOpacity=".2"/></linearGradient>
          <linearGradient id={`${id}-warm`} gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="220"><stop stopColor="#fa7340" stopOpacity="0"/><stop offset=".4" stopColor="#ffaa60" stopOpacity=".7"/><stop offset="1" stopColor="#fff4c0"/></linearGradient>
          <filter id={`${id}-glow`} x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="3"/></filter>
          <filter id={`${id}-soft`} x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="1.5"/></filter>
        </defs>
        <image href="/cooling-interior-v2.png" width="1200" height="600"/>
        <rect width="1200" height="600" fill="#050910" opacity=".18"/>
        <circle cx="920" cy="284" r="79" fill={`url(#${id}-well)`}/>
        <g transform="translate(920 284)"><g ref={rotor} className="cooling-rotor">
          {Array.from({length:48},(_,i)=><path key={i} transform={`rotate(${i*7.5})`} d="M23 -10C40 -29 61 -35 73 -23L74 -16C58 -22 40 -14 24 -5Z" fill={`url(#${id}-blade)`} stroke="#9ba8b0" strokeWidth=".35" strokeOpacity=".23"/>)}
          <circle r="81" fill="none" stroke="#fffbcf" strokeWidth="4" strokeDasharray="165 345" opacity={rpm != null && rpm > 0 ? .95 : .1}/>
        </g></g>
        <circle cx="920" cy="284" r="24" fill={`url(#${id}-hub)`} stroke="#72818a" strokeOpacity=".3"/>
        <circle cx="920" cy="284" r="17" fill="none" stroke="#72818a" strokeOpacity=".15"/>
        <g opacity={rpm != null && rpm > 0 ? .5+(ratio || 0)*.5 : .12}>
          <circle cx="920" cy="284" r="82" fill="none" stroke="#fff8c7" strokeWidth="11" filter={`url(#${id}-glow)`} opacity=".6"/>
          <circle cx="920" cy="284" r="82" fill="none" stroke="#fffeed" strokeWidth="3"/>
        </g>
        <g ref={flow} className="cooling-flow" fill="none" opacity={rpm != null && rpm > 0 ? .55+(ratio || 0)*.45 : 0}>
          {Array.from({length:6},(_,i)=>{
            const cold=`M${864+i*20} ${345+i*2}Q${883+i*12} 313 ${905+i*6} 300`;
            // Short intake cues denote entry into the impeller; not measured chassis streamlines.
            const warm=`M${875+i*21} 215C${875+i*21} 180 ${875+i*21} 160 ${875+i*21} 135L${875+i*21} 5`;
            return <g key={i}>
              {[cold].map((d,j)=><g key={j}><path d={d} stroke={`url(#${id}-cold)`} strokeWidth="5" opacity=".23" filter={`url(#${id}-soft)`}/><path d={d} stroke={`url(#${id}-cold)`} strokeWidth="1.3" opacity=".28"/><path d={d} stroke={`url(#${id}-cold)`} strokeWidth="2.4" strokeDasharray="38 82"/></g>)}
              <path d={warm} stroke={`url(#${id}-warm)`} strokeWidth="6" opacity=".24" filter={`url(#${id}-soft)`}/><path d={warm} stroke={`url(#${id}-warm)`} strokeWidth="1.3" opacity=".4"/><path d={warm} stroke={`url(#${id}-warm)`} strokeWidth="2.7" strokeDasharray="45 75"/>
            </g>;
          })}
        </g>
        <rect width="1200" height="600" fill={`url(#${id}-fade)`}/>
      </svg>
    </div>
    <div className="cooling-readout">
      <span className="eyebrow">ВЕНТИЛЯТОР {index+1} · {fan.name || 'SMC'}</span>
      <div className="cooling-rpm">{rpm == null ? '—' : Math.round(rpm).toLocaleString('ru-RU')}<span>RPM</span></div>
      <p>Оборотов в минуту · датчик macOS</p>
      <div className="cooling-meter" role="meter" aria-label="Обороты относительно максимума" aria-valuemin={0} aria-valuemax={max || undefined} aria-valuenow={rpm ?? undefined} aria-valuetext={rpm == null || max == null ? 'Недоступно' : `${Math.round(ratio!*100)}% от максимума`}>
        {Array.from({length:32},(_,i)=><i className={ratio != null && i/32 < ratio ? 'lit' : ''} key={i}/>)}
      </div>
      <div className="cooling-range"><span>{ratio == null ? '—' : `${Math.round(ratio*100)}% от максимума`}</span><span>{max ? `${max.toLocaleString('ru-RU')} RPM` : 'Максимум неизвестен'}</span></div>
      <div className="cooling-temperature"><span>Температура CPU</span><strong className={temperature != null && temperature >= 90 ? 'hot' : ''}>{temperature == null ? '—' : `${temperature.toFixed(1)} °C`}</strong></div>
      <span className="cooling-motion-note">{reduced ? 'Снижение движения включено в системе' : paused ? 'Анимация приостановлена · измерения идут' : 'Вращение показано в масштабе 1:60'}</span>
    </div>
  </div>;
}

export function Cooling() {
  const s = useChannel(system), state = useChannel(connection);
  const [paused, setPaused] = useState(false), [now, setNow] = useState(Date.now());
  useEffect(() => { const t=setInterval(()=>setNow(Date.now()),2000);return()=>clearInterval(t); },[]);
  const fresh = state === 'connected' && now - s.ts*1000 < 6000 && s.sensor_status === 'available';
  const fans = Array.isArray(s.hardware?.fans) ? s.hardware.fans : [];
  const temperature = fresh && Number.isFinite(s.hardware?.temp?.cpu_temp_avg) ? s.hardware.temp.cpu_temp_avg : null;
  return <section className="panel cooling-panel">
    <div className="panel-title"><div><h2><Fan size={17}/> Система охлаждения</h2><p>Обороты с датчика · поток воздуха показан условно.</p></div><button className="cooling-pause" aria-label={paused ? "Продолжить анимацию" : "Пауза анимации"} aria-pressed={paused} onClick={()=>setPaused(v=>!v)}>{paused ? <Play size={13}/> : <Pause size={13}/>}<span>{paused ? 'Продолжить анимацию' : 'Пауза анимации'}</span></button></div>
    {fans.length ? fans.map((fan:Data,i:number)=><FanScene key={fan.name || i} fan={fan} index={i} running={fresh} paused={paused} temperature={temperature}/>) : <div className="cooling-empty"><Wind size={30}/><p>Данные вентиляторов недоступны</p><small>Анимация появится, когда агент получит показания датчика.</small></div>}
    <p className="cooling-footnote">Условная визуализация: приток к крыльчатке, выхлоп через радиатор к шарниру экрана. Геометрия корпуса и линии потока иллюстративны; направление вращения не измеряется. RPM и температура — с датчиков, скорость вращения показана в масштабе 1:60. Температура и расход воздуха не измеряются.</p>
  </section>;
}
