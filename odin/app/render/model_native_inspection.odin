//! Inspector queries read accepted material owners rather than guessing from source paths.
package render

import ecs "../../ecs"

/// Reports fallback status only when an accepted entry owns a live texture receipt for the role.
model_native_image_fallback :: proc(cache:^Native_Model($R),entity:ecs.Entity_Id,role:int)->(using_fallback,known:bool) {
    if cache==nil || role<0 || role>=5 { return false,false }
    found,status:bool
    for entry,i in cache.batch.entries {
        if entry.entity!=entity { continue }
        if i>=len(cache.receipts) { return false,false }
        index:=cache.receipts[i].textures[role]
        if index<0 || index>=len(cache.textures) || cache.textures[index].native.texture.owner==nil { return false,false }
        image:=cache.textures[index].image
        fallback:=image== -1 || image== -2
        if found && status!=fallback { return false,false }
        found=true;status=fallback
    }
    return status,found
}
