const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
function load(){const context={window:{},document:{documentElement:{lang:''}}};const file=`${__dirname}/i18n.js`;if(fs.existsSync(file))vm.runInNewContext(fs.readFileSync(file,'utf8').replace('/*__TRANSLATIONS__*/',fs.readFileSync(`${__dirname}/locales/en.json`,'utf8')),context);return context;}
test('language changes translate shell copy and preserve interpolated user text',()=>{
 const c=load();assert.equal(typeof c.tr,'function','localization must be available');
 c.setLanguage('en-US');assert.equal(c.tr('Настройки'),'Settings');
 assert.equal(c.tr(['Удалить ', ''], 'Моя папка'),'Delete Моя папка');
 assert.equal(c.document.documentElement.lang,'en');
 c.setLanguage('ru-RU');assert.equal(c.tr('Настройки'),'Настройки');
 c.setLanguage('be-BY');assert.equal(c.document.documentElement.lang,'ru');
});
test('all catalog messages preserve placeholders',()=>{
 const c=load();assert.ok(c.window.slimTranslations,'translation catalog must exist');
 for(const [ru,en] of Object.entries(c.window.slimTranslations))assert.deepEqual([...ru.matchAll(/\{\d+\}/g)].map(m=>m[0]).sort(),[...en.matchAll(/\{\d+\}/g)].map(m=>m[0]).sort(),ru);
});
test('backend messages translate without changing names or error details',()=>{
 const c=load();c.setLanguage('en-US');
 assert.equal(c.trMessage('Снимок сохранён: C:/Мои файлы/a.png'),'Screenshot saved: C:/Мои файлы/a.png');
 assert.equal(c.trMessage('WebView2 не установил расширение: HRESULT 0x1234'),'WebView2 could not install the extension: HRESULT 0x1234');
 assert.equal(c.trMessage('unrecognized error 123'),'unrecognized error 123');
});
test('user titles matching a translation key are rendered verbatim',()=>{
 const c=load();c.setLanguage('en-US');c.document.createElement=()=>({});
 const app=fs.readFileSync(`${__dirname}/app.js`,'utf8');
 vm.runInNewContext(app.slice(app.indexOf('const el='),app.indexOf('const paths='))+'\nwindow.makeElement=el;',c);
 assert.equal(c.window.makeElement('span','folder-name','Настройки').textContent,'Настройки');
});
