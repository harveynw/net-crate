import * as THREE from 'three';


export function loadFloorT(renderer: THREE.WebGLRenderer, repeat: number): THREE.Texture {
    const maxAnisotropy = renderer.capabilities.getMaxAnisotropy();

    const floorT = new THREE.TextureLoader().load( 'textures/FloorsCheckerboard_S_Diffuse.jpg' );
    floorT.colorSpace = THREE.SRGBColorSpace;
    floorT.repeat.set( repeat, repeat );
    floorT.wrapS = floorT.wrapT = THREE.RepeatWrapping;
    floorT.anisotropy = maxAnisotropy;

    return floorT;
}

export function loadFloorN(renderer: THREE.WebGLRenderer, repeat: number): THREE.Texture {
    const maxAnisotropy = renderer.capabilities.getMaxAnisotropy();

    const floorN = new THREE.TextureLoader().load( 'textures/FloorsCheckerboard_S_Normal.jpg' );
    floorN.repeat.set( repeat, repeat );
    floorN.wrapS = floorN.wrapT = THREE.RepeatWrapping;
    floorN.anisotropy = maxAnisotropy;

    return floorN;
}