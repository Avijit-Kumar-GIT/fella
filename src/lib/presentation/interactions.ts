import type { PresentationSpec, PresentationState } from './types';
export interface DisplayFilter {stateKey:string;field:string}
export function controlValueAllowed(spec:PresentationSpec,key:string,value:unknown):boolean {
 const declaration=(spec.elements[spec.root].props as any).contract?.state?.[key];
 if(declaration)return declaration.type==='boolean'?typeof value==='boolean':declaration.options.some((option:any)=>option.value===value);
 return Object.values(spec.elements).some(element=>element.props.stateKey===key&&(element.type==='Checkbox'?typeof value==='boolean':Array.isArray(element.props.options)&&(element.props.options as any[]).some(option=>option.value===value)));
}
export function isPresentationVisible(element:{visible?:unknown},state:PresentationState):boolean {
 const condition=element.visible;
 if(condition===undefined)return true;
 if(typeof condition==='boolean')return condition;
 if(!condition||typeof condition!=='object')return false;
 const record=condition as Record<string,unknown>;
 const key=typeof record.$state==='string'?record.$state.replace(/^\//,''):'';
 return Object.hasOwn(record,'eq')?state[key]===record.eq:Boolean(state[key]);
}
/** Resolve bounded visibility at the host boundary, without executable props. */
export function displayPresentation(spec:PresentationSpec,state:PresentationState):PresentationSpec {
 return {...spec,elements:Object.fromEntries(Object.entries(spec.elements).map(([key,element])=>[key,element.visible===undefined?element:{...element,visible:isPresentationVisible(element,state)}]))};
}
