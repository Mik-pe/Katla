#+test
package app

import "core:testing"
import "core:strings"
import "core:slice"
import editor "../editor"
import resources "../resources"

@(test)
test_model_image_source_identity_survives_owned_history_clone :: proc(t:^testing.T) {
    original:=Gltf_Model{allocator=context.allocator,images=make([]Gltf_Image,1)}
    original.images[0]={name=strings.clone("texture"),mime=strings.clone("image/png"),encoded=make([]byte,2),source_path=strings.clone("models/images/albedo.png"),origin=strings.clone("images/albedo.png")}
    original.images[0].encoded[0]=17; defer gltf_model_destroy(&original)
    cloned:=gltf_model_clone(&original); defer gltf_model_destroy(&cloned)
    testing.expect_value(t,cloned.images[0].source_path,original.images[0].source_path)
    testing.expect(t,raw_data(cloned.images[0].source_path)!=raw_data(original.images[0].source_path))
    testing.expect_value(t,cloned.images[0].origin,"images/albedo.png")
    testing.expect(t,raw_data(cloned.images[0].origin)!=raw_data(original.images[0].origin))
    original.images[0].encoded[0]=32
    testing.expect_value(t,cloned.images[0].encoded[0],byte(17))
    testing.expect_value(t,cloned.images[0].source_path,"models/images/albedo.png")
}

@(test)
test_embedded_glb_and_data_uri_refresh_reextracts_without_replacing_cpu_model :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,".",GLTF_RESOURCE_ROOT),resources.Error.None)
    for path in ([]string{"models/DamagedHelmet.glb","models/UnlitBlend.gltf"}) {
        source,error:=scene_model_prepare(&owner,{path=path}); testing.expect_value(t,error,Gltf_Error.None)
        if error!=.None { continue }; defer scene_model_destroy(&source)
        images:=raw_data(source.model.images)
        testing.expect(t,len(source.model.images)>0)
        for image,index in source.model.images {
            testing.expect_value(t,image.origin,"embedded_buffer_view" if path=="models/DamagedHelmet.glb" else "embedded_data_uri")
            bytes,read_error:=scene_model_image_read(&owner,&source,index); defer delete(bytes)
            testing.expect_value(t,read_error,Gltf_Error.None); testing.expect(t,slice.equal(bytes,image.encoded))
        }
        testing.expect(t,raw_data(source.model.images)==images)
    }
}
