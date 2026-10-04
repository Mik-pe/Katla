#+test
package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import image "../../image"
import km "../../math"
import "core:strings"
import "core:testing"

@(test)
test_inspector_retains_selection_with_internal_array_encoded_image_revision :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    id:=ecs.spawn(&owner.world,struct{transform:app.Scene_Transform,surface:app.Surface_Material}{{km.TRANSFORM_IDENTITY},{roughness=.5,ao=1}})
    decoded,error:=image.texture_image_decode(#load("../render/texture_image_fixtures/rgba.png",[]byte))
    if !testing.expect_value(t,error,image.Texture_Image_Error.None) { return }
    revisions:app.Material_Images;revisions.roles[0]={source={kind=.File,path=strings.clone("fixture.png")},image=decoded}
    ecs.add_component(&owner.world,id,revisions)
    state:State;state_init(&state,&owner);defer state_destroy(&state);testing.expect(t,selection_set(&state,id))
    snapshot,inspect_error:=inspector_read(&state);defer inspector_destroy(&snapshot)
    testing.expect_value(t,inspect_error,editor.Scene_Error.None)
    testing.expect(t,snapshot.has_entity && snapshot.entity==id && state.selection.primary==id)
    surface,transform:bool
    for component in snapshot.components {
        testing.expect(t,component.name!="MaterialImages")
        if component.name=="SurfaceMaterial" { surface=true };if component.name=="SceneTransform" { transform=true }
    }
    testing.expect(t,surface && transform)
}
