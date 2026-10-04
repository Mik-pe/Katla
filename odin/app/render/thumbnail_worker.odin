//! At most four owned background jobs read and decode without touching native GPU state.
package render
import resources "../../resources"
import "core:mem"
import "core:thread"
import "core:sync"
import "base:runtime"

@(private="package")
Thumbnail_Job :: struct { root:resources.Root,path:string,revision:u64,previous,digest:[32]byte,has_previous,unchanged:bool,max_edge:int,image:Texture_Image,error:Thumbnail_Error,done:i32,worker:^thread.Thread }
@(private="package")
thumbnail_background_allocator :: proc()->mem.Allocator { return runtime.heap_allocator() }
@(private="package")
thumbnail_job_ready :: proc(job:^Thumbnail_Job)->bool { return sync.atomic_load(&job.done)!=0 }
@(private="package")
thumbnail_job_start :: proc(job:^Thumbnail_Job)->bool {
    job.worker=thread.create(thumbnail_worker);if job.worker==nil { return false }
    job.worker.data=job;thread.start(job.worker);return true
}
@(private="package")
thumbnail_worker :: proc(worker:^thread.Thread) {
    context.allocator=thumbnail_background_allocator()
    job:=cast(^Thumbnail_Job)worker.data
    bytes,error:=resources.read_bytes(&job.root,job.path,TEXTURE_IMAGE_MAX_ENCODED_BYTES)
    if error!=.None { job.error={resource=error} } else {
        job.digest=thumbnail_digest(bytes)
        job.unchanged=job.has_previous && job.previous==job.digest
        if !job.unchanged {
            image,image_error:=texture_image_decode(bytes)
            if image_error!=.None { job.error={image=image_error} } else {
                resize_error:Texture_Image_Error
                job.image,resize_error=thumbnail_resize(&image,job.max_edge,context.allocator)
                if resize_error!=.None { job.error={image=resize_error} }
                if raw_data(job.image.pixels)!=raw_data(image.pixels) { texture_image_destroy(&image) }
            }
        }
        delete(bytes,job.root.allocator)
    }
    sync.atomic_store(&job.done,1)
}
@(private="package")
thumbnail_job_destroy :: proc(job:^Thumbnail_Job,allocator:mem.Allocator) {
    if job.worker!=nil { thread.join(job.worker);thread.destroy(job.worker) }
    texture_image_destroy(&job.image);resources.root_destroy(&job.root);free(job,allocator)
}
