const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const privacy=fs.readFileSync(`${__dirname}/privacy.js`,'utf8');
const app=fs.readFileSync(`${__dirname}/app.js`,'utf8');
test('drop placement resolves before and after rows within the visible list',()=>{
    const context={};vm.runInNewContext(app.slice(app.indexOf('function dropBefore('),app.indexOf('function renderTabs()')),context);
    const a={dataset:{tabId:'1'},getBoundingClientRect:()=>({top:20,height:40}),nextElementSibling:null};
    const b={dataset:{tabId:'2'}};a.nextElementSibling=b;
    assert.equal(context.dropBefore(a,25,'tabId'),1);assert.equal(context.dropBefore(a,55,'tabId'),2);
    a.nextElementSibling=null;assert.equal(context.dropBefore(a,55,'tabId'),null);
});
test('tab and folder keyboard moves stay in their own group and respect edges',()=>{
    const messages=[],context={post:(cmd,data)=>messages.push({cmd,...data}),state:{tabs:[{id:9,url:'slim://newtab'},{id:1},{id:20,folder_id:3},{id:2},{id:3}],folders:[{id:3},{id:5},{id:7}]}};
    vm.runInNewContext(app.slice(app.indexOf('function moveTabBy('),app.indexOf('function dropBefore(')),context);
    context.moveTabBy(1,-1);assert.equal(messages.length,0);
    context.moveTabBy(1,1);assert.equal(messages[0].cmd,'tab_reorder');assert.equal(messages[0].before,3);assert.equal(messages[0].folder,null);
    context.moveTabBy(20,1);assert.equal(messages.length,1);
    context.moveFolderBy(5,-1);assert.equal(messages.at(-1).before,3);
    context.moveFolderBy(5,1);assert.equal(messages.at(-1).before,null);
    context.moveFolderBy(7,1);assert.equal(messages.length,3);
});
test('snapshots cannot detach drag nodes or rebuild the sidebar during resize',()=>{
    const context={sidebarDrag:{kind:'tab',id:1},document:{documentElement:{classList:{contains:()=>true}}}};
    vm.runInNewContext(app.slice(app.indexOf('function renderTabs()'),app.indexOf('function renderMainMenu()')),context);
    assert.doesNotThrow(()=>context.renderTabs());context.sidebarDrag=null;assert.doesNotThrow(()=>context.renderTabs());
});
test('sidebar position motion uses transforms and honours reduced motion',()=>{
    const animations=[],node={dataset:{motionKey:'tab-1'},getClientRects:()=>[1],getBoundingClientRect:()=>({top:80,left:0}),animate:(frames,options)=>animations.push({frames,options})};
    const context={reducedMotion:{matches:false},sidebarDrag:null,document:{querySelectorAll:()=>[node]}};
    vm.runInNewContext(app.slice(app.indexOf('function animateSidebarChanges('),app.indexOf("reducedMotion.addEventListener(")),context);
    context.animateSidebarChanges(new Map([['tab-1',{top:40,left:0}]]));assert.equal(animations.length,1);assert.equal(animations[0].frames[0].transform,'translate(0px,-40px)');
    context.reducedMotion.matches=true;context.animateSidebarChanges(new Map([['tab-1',{top:40,left:0}]]));assert.equal(animations.length,1);
});
function resizeHarness(layout='split',pageLeft=222){
    const handlers={},messages=[],frames=new Map(),styles=new Map(),classes=new Set();let nextFrame=1;
    const handle={addEventListener:(type,fn)=>handlers[type]=fn,setPointerCapture(){},hasPointerCapture:()=>true,releasePointerCapture(){},style:{},setAttribute(){}};
    const address={blur(){address.blurred=true;}};
    const context={state:{pageLeft,settings:{BrowserLayout:layout}},innerWidth:1200,post:(cmd,data)=>messages.push({cmd,...data}),
        $:id=>id==='sidebar-resizer'?handle:id==='address'?address:{style:{}},
        document:{activeElement:address,documentElement:{classList:{add:c=>classes.add(c),remove:c=>classes.delete(c),contains:c=>classes.has(c)},style:{setProperty:(key,value)=>styles.set(key,value)}}},
        requestAnimationFrame:fn=>{const id=nextFrame++;frames.set(id,fn);return id;},cancelAnimationFrame:id=>frames.delete(id),window:{addEventListener(){}},Math};
    context.applySidebarWidth=width=>{context.state.pageLeft=width;styles.set('--sidebar-width',`${width}px`);};
    vm.runInNewContext(privacy.slice(privacy.indexOf('function setupSidebarResize()')),context);
    context.setupSidebarResize();
    const event=(values={})=>({button:0,pointerId:1,clientX:219,preventDefault(){},...values});
    return {context,handle,handlers,messages,styles,classes,address,event,flush(){for(const [id,fn]of [...frames]){frames.delete(id);fn();}},frames};
}
test('sidebar tracks grab offset locally once per frame and flushes final commit',()=>{
    const h=resizeHarness();h.handlers.pointerdown(h.event());
    h.handlers.pointermove(h.event({clientX:280}));h.handlers.pointermove(h.event({clientX:300}));
    assert.equal(h.frames.size,1);assert.equal(h.messages.length,0);h.flush();
    assert.equal(h.styles.get('--sidebar-width'),'303px');assert.equal(h.messages[0].value,303);assert.equal(h.messages[0].commit,false);
    h.handlers.pointermove(h.event({clientX:330}));h.handlers.pointerup(h.event({clientX:330}));
    assert.equal(h.messages.at(-1).value,333);assert.equal(h.messages.at(-1).commit,true);assert.equal(h.frames.size,0);assert.equal(h.classes.has('sidebar-resizing'),false);
});
test('sidebar resize clears address focus and restores cursor after capture loss',()=>{
    const h=resizeHarness();h.handlers.pointerdown(h.event());assert.equal(h.address.blurred,true);
    h.handlers.pointermove(h.event({clientX:1000}));h.handlers.lostpointercapture(h.event());
    assert.equal(h.classes.has('sidebar-resizing'),false);assert.equal(h.messages.at(-1).value,480);assert.equal(h.messages.at(-1).commit,true);
});
test('keyboard separator obeys layout and viewport bounds',()=>{
    const h=resizeHarness('compact',150);h.context.innerWidth=540;
    h.handlers.keydown(h.event({key:'End'}));assert.equal(h.messages.at(-1).value,100);
    h.handlers.keydown(h.event({key:'Home'}));assert.equal(h.messages.at(-1).value,64);
});
