// Russian source messages are keys. Tagged templates keep user content out of translation.
let interfaceLanguage='ru-RU';
function setLanguage(value){interfaceLanguage=value==='en-US'?'en-US':'ru-RU';document.documentElement.lang=interfaceLanguage==='en-US'?'en':'ru';}
function tr(message,...values){
    const key=Array.isArray(message)?message.reduce((s,part,i)=>s+(i?`{${i-1}}`:'')+part,''):message;
    const translated=interfaceLanguage==='en-US'?(window.slimTranslations[key]??key):key;
    return values.length?translated.replace(/\{(\d+)\}/g,(_,i)=>String(values[Number(i)])):translated;
}
function trMessage(message){
    if(interfaceLanguage!=='en-US'||!message)return message;
    if(window.slimTranslations[message])return window.slimTranslations[message];
    for(const [source,target]of Object.entries(window.slimTranslations)){
        if(!source.includes('{0}'))continue;
        const pattern=source.split(/\{\d+\}/).map(s=>s.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')).join('([\\s\\S]*?)');
        const match=new RegExp(`^${pattern}$`).exec(message);
        if(match)return target.replace(/\{(\d+)\}/g,(_,i)=>match[Number(i)+1]);
    }
    return message;
}
let staticLabels;
function localizeStatic(){
    // Capture only original shell markup, before any folders or page titles exist.
    staticLabels??=[...document.querySelectorAll('[aria-label],[placeholder]')].flatMap(node=>
        ['aria-label','placeholder'].filter(attr=>node.hasAttribute(attr)).map(attr=>({node,attr,source:node.getAttribute(attr)})));
    for(const {node,attr,source}of staticLabels)node.setAttribute(attr,tr(source));
    const label=document.querySelector('.new-tab-label');if(label)label.textContent=tr('Новая вкладка');
}
window.slimTranslations=/*__TRANSLATIONS__*/;
