// Проверка меток версий файлов интерфейса для сборки.
// Спецификация: specs/ui-cache-tag-check.md.
// Запуск: node scripts/check-ui-cache-tags.js [базовая-ветка-или-ссылка]
// Без параметра база берётся из окружения сборки: целевая ветка запроса на
// слияние, иначе предыдущее состояние текущей ветки.
'use strict';
const {execFileSync}=require('node:child_process');
const {checkCacheTags}=require('./ui-cache-tags.js');

function git(args){
  return execFileSync('git',args,{encoding:'utf8',maxBuffer:64*1024*1024});
}

function resolveBase(){
  const fromArgument=process.argv[2];
  if(fromArgument)return fromArgument;
  const pullRequestBase=process.env.GITHUB_BASE_REF;
  if(pullRequestBase)return `origin/${pullRequestBase}`;
  const pushBefore=process.env.GITHUB_EVENT_BEFORE;
  if(pushBefore&&!/^0+$/.test(pushBefore))return pushBefore;
  return 'HEAD~1';
}

function ensureAvailable(reference){
  try{
    git(['rev-parse','--verify',`${reference}^{commit}`]);
    return reference;
  }catch(_){
    // Неполная выгрузка истории: запрашиваем недостающее и пробуем ещё раз.
    try{
      git(['fetch','--no-tags','--depth=50','origin',reference.replace(/^origin\//,'')]);
      git(['rev-parse','--verify',`${reference}^{commit}`]);
      return reference;
    }catch(error){
      console.error(`Не удалось получить состояние базы ${reference}: ${error.message}`);
      process.exit(2);
    }
  }
}

function main(){
  const base=ensureAvailable(resolveBase());
  const raw=git(['diff','--name-status',`${base}...HEAD`,'--','apps/desktop/ui']).trim();
  const changes=raw?raw.split('\n').map(line=>{
    const [status,...rest]=line.split('\t');
    return {status:status[0],path:rest[rest.length-1]};
  }):[];
  if(!changes.length){
    console.log('Файлы интерфейса не менялись - проверять метки нечего.');
    return;
  }
  const cache=new Map();
  const readHost=(path,side)=>{
    const key=`${side}:${path}`;
    if(!cache.has(key)){
      try{
        cache.set(key,side==='base'?git(['show',`${base}:${path}`]):git(['show',`HEAD:${path}`]));
      }catch(_){
        cache.set(key,null);
      }
    }
    return cache.get(key);
  };
  const violations=checkCacheTags(changes,readHost);
  console.log(`Проверено изменённых файлов интерфейса: ${changes.length}`);
  if(!violations.length){
    console.log('Метки версий подняты у всех изменённых файлов.');
    return;
  }
  console.error('Изменены файлы интерфейса, но метка версии осталась прежней:');
  for(const violation of violations){
    console.error(`  ${violation.file} - метка ${violation.tag} в ${violation.host}`);
  }
  console.error('Поднимите метку ?v= у этих файлов, иначе после обновления останется старая копия из кэша.');
  process.exit(1);
}

main();
