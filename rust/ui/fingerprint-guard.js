(()=>{"use strict";if(!/^https?:$/.test(location.protocol))return;
// Block high-entropy readback, retaining rendering. Applies only on reload when
// the user explicitly enables suspected-fingerprinting protection.
const denied=()=>{throw new DOMException('Readback disabled by Slim Browser privacy settings','NotAllowedError')};
for(const [target,names]of [[globalThis.HTMLCanvasElement?.prototype,['toDataURL','toBlob']],[globalThis.CanvasRenderingContext2D?.prototype,['getImageData']],[globalThis.WebGLRenderingContext?.prototype,['readPixels']],[globalThis.WebGL2RenderingContext?.prototype,['readPixels']],[globalThis.OfflineAudioContext?.prototype,['startRendering']]]){if(!target)continue;for(const name of names){if(typeof target[name]==='function'){try{Object.defineProperty(target,name,{value:denied,configurable:true,writable:true})}catch{}}}}
})();
