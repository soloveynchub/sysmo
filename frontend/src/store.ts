import {useSyncExternalStore} from 'react';
export type Data=Record<string,any>;
function channel<T>(initial:T){let value=initial;const listeners=new Set<()=>void>();return {get:()=>value,set:(v:T)=>{value=v;listeners.forEach(f=>f())},sub:(f:()=>void)=>{listeners.add(f);return()=>{listeners.delete(f)}}}}
export const system=channel<Data>({});export const processes=channel<Data[]>([]);export const docker=channel<Data>({status:'checking',containers:[]});export const connection=channel('connecting');export const samples=channel<Data[]>([]);
export function useChannel<T>(c:ReturnType<typeof channel<T>>){return useSyncExternalStore(c.sub,c.get)}
let socket:WebSocket|undefined;let retry=0;let latest:Data|undefined;let pending:Data|undefined;let processMap=new Map<string,Data>();
function publish(d:Data){if(d.system){system.set(d.system);const s=samples.get();if(!s.length||d.system.ts>s[s.length-1].ts)samples.set([...s,d.system].slice(-900))}processes.set([...processMap.values()]);if(d.docker)docker.set(d.docker)}
function connect(){connection.set(retry?'reconnecting':'connecting');socket=new WebSocket(`ws://${location.host}/api/live`);socket.onopen=()=>{retry=0;connection.set('connected')};socket.onmessage=e=>{const d=JSON.parse(e.data);if(d.type==='snapshot')processMap=new Map(d.processes.map((p:Data)=>[p.id,p]));else{d.changed?.forEach((p:Data)=>processMap.set(p.id,{...processMap.get(p.id),...p}));d.removed?.forEach((id:string)=>processMap.delete(id))}latest=d;if(document.hidden){pending=d;return}publish(d)};socket.onclose=()=>{connection.set('reconnecting');setTimeout(connect,Math.min(1000*2**retry++,10000))};socket.onerror=()=>socket?.close()}
connect();document.addEventListener('visibilitychange',()=>{if(!document.hidden&&pending){publish(pending);pending=undefined;window.dispatchEvent(new Event('monitor-resume'))}});
export function getPath(v:Data,key:string):number|null{let p:any=v;for(const k of key.split('.'))p=p?.[k];return typeof p==='number'&&Number.isFinite(p)?p:null}
export function getLatest(){return latest}
