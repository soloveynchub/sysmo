// Preserve previously shared preview links while using the main application.
const targetScreen=new URLSearchParams(location.search).get('screen')||'Накопитель';
location.replace('/#'+encodeURIComponent(['Накопитель','Docker','Процессы','Освободить ресурсы'].includes(targetScreen)?targetScreen:'Накопитель'));
